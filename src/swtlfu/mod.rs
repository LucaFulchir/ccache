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

use crate::results::InsertResult;
use crate::user;
use bitvec::prelude::*;

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
/// 

pub struct SWTLFUShared<E, K, V, Cid, CidCtr, Umeta, Fscan, HB>
where
    E: user::EntryT<K, V, CidCtr, Umeta>,
    V: Sized,
    Cid: crate::cid::Cid,
    CidCtr: counter::CidCounter<Cid>,
    Umeta: user::Meta<V>,
    Fscan: Sized + Copy + Fn(::std::ptr::NonNull<E>),
    HB: ::std::hash::BuildHasher,
{
    _reset_counters: counter::Full32,
    _window: crate::lru::LRUShared<E, K, V, CidCtr, Umeta, Fscan, HB>,
    _slru: crate::slru::SLRUShared<E, K, V, CidCtr, Umeta, Fscan, HB>,
    _cid: ::std::marker::PhantomData<Cid>,
}

impl<
        E: user::EntryT<K, V, CidCtr, Umeta>,
        K: ::std::hash::Hash + Clone + Eq,
        V,
        Cid: crate::cid::Cid,
        CidCtr: counter::CidCounter<Cid>,
        Umeta: user::Meta<V>,
        Fscan: Sized + Copy + Fn(::std::ptr::NonNull<E>),
        HB: ::std::hash::BuildHasher,
    > SWTLFUShared<E, K, V, Cid, CidCtr, Umeta, Fscan, HB>
{
    pub fn new_standard(
        window_cid: CidCtr,
        probation_cid: CidCtr,
        protected_cid: CidCtr,
        entries: usize,
        access_scan: Fscan
    ) -> SWTLFUShared<E, K, V, Cid, CidCtr, Umeta, Fscan, HB> {
        let floor_window_entries = ((entries as f64) * 0.01) as usize;
        let window_entries = ::std::cmp::max(1, floor_window_entries);

        let main_entries = entries - window_entries;

        let (probation_entries, protected_entries) =
            match ((main_entries as f64) * 0.2) as usize {
                0 => {
                    if main_entries <= 1 {
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
            access_scan)
    }
    pub fn new(
        window: (usize, CidCtr),
        probation: (usize, CidCtr),
        protected: (usize, CidCtr),
        access_scan: Fscan,
    ) -> SWTLFUShared<E, K, V, Cid, CidCtr, Umeta, Fscan, HB> {

        SWTLFUShared {
            _reset_counters: counter::Full32::default(),
            _window: 
                crate::lru::LRUShared::<E, K, V, CidCtr, Umeta, Fscan, HB>::new(
                    window.0,
                    window.1,
                    access_scan,
                ),
            _slru:
                crate::slru::SLRUShared::<E, K, V, CidCtr, Umeta, Fscan, HB>::new(
                    probation,
                    protected,
                    access_scan,
                ),
            _cid: ::std::marker::PhantomData,
        }
    }
    /*
    pub fn insert_shared(
        &mut self,
        hmap: &mut ::std::collections::HashMap<K, E, HB>,
        maybe_old_entry: Option<E>,
        key: &K,
    ) -> InsertResultShared<E, K> {
        let just_inserted = hmap.get_mut(&key).unwrap();
        *just_inserted.get_cache_id_mut() = self._cache_id;

        match maybe_old_entry {
            None => {
                just_inserted.user_on_insert(None);
            }
            Some(old_entry) => {}
        }
    }
    */
}
