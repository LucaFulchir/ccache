/*
 * Copyright 2021 Luca Fulchir <luker@fenrirproject.org>
 *
 * Licensed under the Apache License, Version 2.0 (the "License");
 * you may not use this file except in compliance with the License.
 * You may obtain a copy of the License at
 *
 *   http://www.apache.org/licenses/LICENSE-2.0
 *
 * Unless required by applicable law or agreed to in writing, software
 * distributed under the License is distributed on an "AS IS" BASIS,
 * WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
 * See the License for the specific language governing permissions and
 * limitations under the License.
 */

mod counter;

use crate::hashmap;
use crate::results::{InsertResult, InsertResultShared};
use crate::user;

#[derive(PartialEq, Eq)]
enum ScanStatus {
    Stopped,
    Running,
}

struct ScanScan<'a, F: ?Sized, E>
where
    Box<F>: Fn(::std::ptr::NonNull<E>) -> (),
{
    // Main scan function: will keep scanning all wtlfu continuously
    // should never be stopped
    wtlfu_scan: ::std::boxed::Box<F>,
    // user accitional scan function. can be stopped
    user_scan:
        ::std::boxed::Box<Option<&'a dyn Fn(::std::ptr::NonNull<E>) -> ()>>,
    status: ::std::boxed::Box<ScanStatus>,
    _entry: ::std::marker::PhantomData<E>,
}

/// TinyLFU is a series of counters and an SLRU cache.
/// W-TinyLFU adds another LRU window in front of all of this.
///
/// The Window is 1% of the total cache, while the main SLRU is divided in
/// 20% probation, 80% protected cache.
///
/// WTLFU keeps a bloom filter of the Window. If a entry in the Window receives
/// a HIT it is moved to the SLRU, where its access are counted better.  
/// This second counters can be limited to a few bits, but they are kept
/// separate from the bloom filter for efficiency.  
///
/// If the cache is designed for X entries, after X inserts we have
/// to halve all counters. This implies blocking all the cache, which I did
/// not found acceptable.
///
/// Therefore here we have the Scan-Window-TinyLFU, which is your average WLTFU
/// but we keep full counters for all elements.  
/// Due to memory aligment we are already wasting the space for every entry
/// anyway.
///
/// The "Scan" part works simply by tracking the generation of the counter
/// (`Day`/`Night`) and if the generation is not the current one, the counter
/// is halved. To assure that all counters are halved every X inserts,
/// every get/insert will scan just one more element.
///
/// The reason why we are already wasting the memory and we can repurpose it
/// to counters is that our "Shared" caches are designed to be composable
/// over the same hashmap. This saves us a lot of delete/insert when each
/// element should be moved between caches, but we now need something that
/// tracks to which cache an entry belongs to (the `Cid` -- Cache Id)

pub struct SWTLFUShared<'a, Hmap, E, K, V, CidT, CidCtr, Umeta, HB>
where
    Hmap: hashmap::HashMap<E, K, V, CidCtr, Umeta, HB>,
    E: user::EntryT<K, V, CidCtr, Umeta>,
    K: user::Hash,
    V: user::Val,
    CidT: user::Cid,
    CidCtr: counter::CidCounter<CidT>,
    Umeta: user::Meta<V>,
    HB: ::std::hash::BuildHasher + Default,
{
    _window: crate::lru::LRUShared<'a, Hmap, E, K, V, CidCtr, Umeta, HB>,
    _slru: crate::slru::SLRUShared<'a, Hmap, E, K, V, CidCtr, Umeta, HB>,
    _entries: usize,
    _random: [usize; 2],
    _generation: ::std::boxed::Box<counter::Generation>,
    _cid_window: CidT,
    _cid_probation: CidT,
    _cid_protected: CidT,
    _hmap: ::std::marker::PhantomData<Hmap>,
    _cid: ::std::marker::PhantomData<CidT>,
    _scan: ScanScan<'a, dyn Fn(::std::ptr::NonNull<E>) + 'a, E>,
}

// FIXME: lifetimes here seem all wrong, we're doing some thing wrong...
impl<
        'a,
        Hmap: hashmap::HashMap<E, K, V, CidCtr, Umeta, HB> + 'a,
        E: user::EntryT<K, V, CidCtr, Umeta> + 'a,
        K: user::Hash + 'a,
        V: user::Val + 'a,
        CidT: user::Cid + 'a,
        CidCtr: counter::CidCounter<CidT> + 'a,
        Umeta: user::Meta<V> + 'a,
        HB: ::std::hash::BuildHasher + Default + 'a,
    > SWTLFUShared<'a, Hmap, E, K, V, CidT, CidCtr, Umeta, HB>
{
    pub fn new_standard(
        window_cid: CidT,
        probation_cid: CidT,
        protected_cid: CidT,
        entries: usize,
        access_scan: Option<&'a dyn Fn(::std::ptr::NonNull<E>) -> ()>,
    ) -> Self {
        // We keep at least one element in each cache

        let floor_window_entries = ((entries as f64) * 0.01) as usize;
        let window_entries = ::std::cmp::max(1, floor_window_entries);

        let main_entries = entries - window_entries;

        let (probation_entries, protected_entries) =
            match ((main_entries as f64) * 0.2) as usize {
                0 => {
                    if main_entries <= 2 {
                        (1, 1)
                    } else {
                        (1, main_entries - 1)
                    }
                }
                x @ _ => (x, entries - x),
            };
        SWTLFUShared::new(
            (window_entries, window_cid),
            (probation_entries, probation_cid),
            (protected_entries, protected_cid),
            access_scan,
        )
    }
    pub fn new(
        window: (usize, CidT),
        probation: (usize, CidT),
        protected: (usize, CidT),
        access_scan: Option<&'a dyn Fn(::std::ptr::NonNull<E>) -> ()>,
    ) -> Self {
        // make sure there is at least one element per cache
        // This assures us that there are at least 3 elements
        // and thus we can safely generate 3 different indexes during get/insert
        let real_window = if window.0 == 0 { (1, window.1) } else { window };
        let real_probation = if protected.0 == 0 {
            (1, probation.1)
        } else {
            probation
        };
        let real_protected = if protected.0 == 0 {
            (1, protected.1)
        } else {
            protected
        };
        let gen = ::std::boxed::Box::<counter::Generation>::new(
            counter::Generation::default(),
        );
        let mut sw_tlfu = SWTLFUShared {
            _window: crate::lru::LRUShared::<
                'a,
                Hmap,
                E,
                K,
                V,
                CidCtr,
                Umeta,
                HB,
            >::new(
                real_window.0, CidCtr::new(real_window.1), None
            ),
            _scan: ScanScan {
                wtlfu_scan: ::std::boxed::Box::new(
                    move |_e: ::std::ptr::NonNull<E>| {},
                ),
                user_scan: ::std::boxed::Box::new(access_scan),
                status: ::std::boxed::Box::new(ScanStatus::Stopped),
                _entry: ::std::marker::PhantomData,
            },
            _slru: crate::slru::SLRUShared::<
                'a,
                Hmap,
                E,
                K,
                V,
                CidCtr,
                Umeta,
                HB,
            >::new(
                (real_probation.0, CidCtr::new(real_probation.1)),
                (real_protected.0, CidCtr::new(real_protected.1)),
                None,
            ),
            _entries: real_window.0 + real_probation.0 + real_protected.0,
            _random: [::rand::random::<usize>(), ::rand::random::<usize>()],
            _generation: gen,
            _cid_window: real_window.1,
            _cid_probation: real_probation.1,
            _cid_protected: real_protected.1,
            _hmap: ::std::marker::PhantomData,
            _cid: ::std::marker::PhantomData,
        };
        sw_tlfu.set_main_scanf_once();
        sw_tlfu
    }
    fn set_main_scanf_once(&mut self) {
        // trick rust into ignoring lifetimes through NonNull
        unsafe {
            let nn_user_scan: ::std::ptr::NonNull<
                Option<&'a dyn Fn(::std::ptr::NonNull<E>) -> ()>,
            > = (&*self._scan.user_scan).into();
            let nn_status: ::std::ptr::NonNull<ScanStatus> =
                (&*self._scan.status).into();
            self._scan.wtlfu_scan = ::std::boxed::Box::new(
                self.continuous_scan(nn_status.as_ref(), nn_user_scan.as_ref()),
            );
            let nn_wtlfu_scan: ::std::ptr::NonNull<
                dyn Fn(::std::ptr::NonNull<E>) -> (),
            > = (&*self._scan.wtlfu_scan).into();
            self._window.set_scanf(Some(nn_wtlfu_scan.as_ref()));
            self._slru.set_scanf(Some(nn_wtlfu_scan.as_ref()));
        }
        self._window.start_scan();
    }
    pub fn set_scanf(
        &mut self,
        access_scan: Option<&'a dyn Fn(::std::ptr::NonNull<E>) -> ()>,
    ) {
        *self._scan.user_scan = access_scan;
    }
    // return ALL indexes chosen deterministically based on the one in input
    // make sure they are different and that the initial index is included
    fn det_idx(&self, idx: usize) -> [usize; 3] {
        let first = {
            let tmp = (idx ^ self._random[0]) % self._entries;
            if tmp != idx {
                tmp
            } else {
                (tmp + 1) % self._entries
            }
        };
        let second: usize = {
            let mut tmp = (idx ^ self._random[1]) % self._entries;
            loop {
                if tmp != idx && tmp != first {
                    break tmp;
                }
                tmp = (tmp - 1) % self._entries;
            }
        };
        [idx, first, second]
    }
    // choose the entry to evict
    fn choose_evict(&self, hmap: &mut Hmap, idx: usize) -> usize {
        let (mut toevict, mut minfreq) = (idx, usize::MAX);
        for idx in self.det_idx(idx).iter() {
            match hmap.get_index(*idx) {
                None => {}
                Some(entry) => {
                    let freq = entry.get_cache_id().get_counter() as usize;
                    if freq < minfreq {
                        minfreq = freq;
                        toevict = *idx;
                    }
                }
            }
        }
        toevict
    }
    pub fn insert_shared(
        &mut self,
        hmap: &mut Hmap,
        maybe_old_entry: Option<&mut E>,
        new_entry_idx: usize,
    ) -> InsertResultShared<E> {
        // TODO: scan one more entry
        let just_inserted = hmap.get_index_mut(new_entry_idx).unwrap();
        let cid_j_i = just_inserted.get_cache_id().get_cid();
        if cid_j_i == self._cid_window {
            // promote it to main
            self._window.remove_shared(just_inserted);
            match maybe_old_entry {
                None => self._slru.insert_shared(hmap, None, new_entry_idx),
                Some(old_entry) => {
                    if old_entry.get_cache_id().get_cid() == self._cid_window {
                        self._slru.insert_shared(
                            hmap,
                            Some(old_entry),
                            new_entry_idx,
                        )
                    } else {
                        self._slru.insert_shared(hmap, None, new_entry_idx)
                    }
                }
            }
        } else if (cid_j_i == self._cid_probation)
            || (cid_j_i == self._cid_protected)
        {
            match maybe_old_entry {
                None => self._slru.insert_shared(hmap, None, new_entry_idx),
                Some(old_entry) => {
                    let old_cid = old_entry.get_cache_id().get_cid();
                    if (old_cid == self._cid_probation)
                        || (old_cid == self._cid_protected)
                    {
                        self._slru.insert_shared(
                            hmap,
                            Some(old_entry),
                            new_entry_idx,
                        )
                    } else {
                        self._slru.insert_shared(hmap, None, new_entry_idx)
                    }
                }
            }
        } else {
            // put in window
            // evicted from window get a second chance
            let res_window = match maybe_old_entry {
                None => self._window.insert_shared(hmap, None, new_entry_idx),
                Some(old_entry) => {
                    if old_entry.get_cache_id().get_cid() == self._cid_window {
                        self._window.insert_shared(
                            hmap,
                            Some(old_entry),
                            new_entry_idx,
                        )
                    } else {
                        self._window.insert_shared(hmap, None, new_entry_idx)
                    }
                }
            };
            match res_window {
                InsertResultShared::OldEntry { evicted } => {
                    // This means that we had a clash and we are forced to
                    // report the evicted as such, it makes
                    // no sense to try to add it again to the
                    // SLRU since it would benerate more clashes
                    InsertResultShared::OldEntry { evicted }
                }
                InsertResultShared::OldTailPtr { evicted } => {
                    let evicted_idx =
                        unsafe { hmap.index_from_entry(&*evicted.as_ptr()) };
                    let to_evict_idx = self.choose_evict(hmap, evicted_idx);
                    let to_evict = hmap.get_index(to_evict_idx).unwrap();
                    if to_evict.get_cache_id().get_cid() == self._cid_window {
                        return InsertResultShared::Success;
                    }
                    self._slru.insert_shared(hmap, None, evicted_idx)
                }
                InsertResultShared::Success => InsertResultShared::Success,
            }
        }
    }
    pub fn clear_shared(&mut self) {
        self._window.clear_shared();
        self._slru.clear_shared();
    }
    pub fn remove_shared(&mut self, entry: &E) {
        let res = if entry.get_cache_id().get_cid() == self._cid_window {
            self._window.remove_shared(entry)
        } else {
            self._slru.remove_shared(entry)
        };
        self.update_scan_status();
        res
    }
    pub fn get_cache_ids(&self) -> [CidT; 3] {
        [self._cid_window, self._cid_probation, self._cid_protected]
    }
    pub fn on_get(&mut self, entry: &mut E) {
        if entry.get_cache_id().get_cid() == self._cid_window {
            self._window.on_get(entry);
        } else {
            self._slru.on_get(entry);
        }
        self.update_scan_status();
    }
    pub fn start_scan(&mut self) {
        *self._scan.status = ScanStatus::Running;
    }
    pub fn is_scan_running(&self) -> bool {
        return *self._scan.status != ScanStatus::Stopped;
    }
    fn update_scan_status(&mut self) {
        // scanning is always running for wtlfu
        // but the user can stop its own can function
        match self._window.is_scan_running() {
            true => {}
            false => {
                self._slru.start_scan();
            }
        };
        match self._slru.is_scan_running() {
            true => {}
            false => {
                self._window.start_scan();
                *self._scan.status = ScanStatus::Stopped;
            }
        }
    }
    pub fn capacity(&self) -> usize {
        self._entries
    }
    pub fn len(&self) -> usize {
        self._window.len() + self._slru.len()
    }
    fn continuous_scan(
        &self,
        status: &'a ScanStatus,
        fscan: &'a Option<&'a dyn Fn(::std::ptr::NonNull<E>) -> ()>,
    ) -> impl Fn(::std::ptr::NonNull<E>) -> () + 'a {
        let generation: ::std::ptr::NonNull<counter::Generation> =
            (&*self._generation).into();
        move |entry: ::std::ptr::NonNull<E>| -> () {
            unsafe {
                if *generation.as_ptr()
                    != (*entry.as_ref()).get_cache_id().get_generation()
                {
                    entry.as_ref().get_cache_id().halve();
                    entry.as_ref().get_cache_id().flip_generation();
                } else {
                    // do nothing
                }
            }
            if fscan.is_some() && *status == ScanStatus::Running {
                (fscan.unwrap())(entry)
            }
        }
    }
}
