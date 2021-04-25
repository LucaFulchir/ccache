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

use crate::results::{InsertResult, InsertResultShared};
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
    _window: crate::lru::LRUShared<E, K, V, CidCtr, Umeta, Fscan, HB>,
    _slru: crate::slru::SLRUShared<E, K, V, CidCtr, Umeta, Fscan, HB>,
    _entries: usize,
    _generation: counter::Generation,
    _cid_window: Cid,
    _cid_probation: Cid,
    _cid_protected: Cid,
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
        window_cid: Cid,
        probation_cid: Cid,
        protected_cid: Cid,
        entries: usize,
        access_scan: Fscan
    ) -> SWTLFUShared<E, K, V, Cid, CidCtr, Umeta, Fscan, HB> {
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
            access_scan)
    }
    pub fn new(
        window: (usize, Cid),
        probation: (usize, Cid),
        protected: (usize, Cid),
        access_scan: Fscan,
    ) -> SWTLFUShared<E, K, V, Cid, CidCtr, Umeta, Fscan, HB> {

        SWTLFUShared {
            _window: 
                crate::lru::LRUShared::<E, K, V, CidCtr, Umeta, Fscan, HB>::new(
                    window.0,
                    CidCtr::new(window.1),
                    access_scan,
                ),
            _slru:
                crate::slru::SLRUShared::<E, K, V, CidCtr, Umeta, Fscan, HB>::new(
                    (probation.0, CidCtr::new(probation.1)),
                    (protected.0, CidCtr::new(protected.1)),
                    access_scan,
                ),
            _entries: window.0 + probation.0 + protected.0,
            _generation: counter::Generation::default(),
            _cid_window: window.1,
            _cid_probation: probation.1,
            _cid_protected: protected.1,
            _cid: ::std::marker::PhantomData,
        }
    }
    pub fn insert_shared(
        &mut self,
        hmap: &mut ::std::collections::HashMap<K, E, HB>,
        maybe_old_entry: Option<E>,
        key: &K,
    ) -> InsertResultShared<E> {
        let just_inserted = hmap.get_mut(&key).unwrap();

        match maybe_old_entry {
            None => {
                let cid_j_i = just_inserted.get_cache_id().get_cid();
                if cid_j_i == self._cid_window {
                    // promote it to main
                    self._window.remove_shared(just_inserted);
                    self._slru.insert_shared(hmap, None, key)
                } else if (cid_j_i == self._cid_probation) ||
                    (cid_j_i == self._cid_protected) {
                    // let the main shared handle this
                    self._slru.insert_shared(hmap, None, key);
                    InsertResultShared::Success 
                } else {
                    // put in window
                    match self._window.insert_shared(hmap, None, key) {
                        // if we have evicted, they are given a second chance
                        InsertResultShared::OldTailPtr{clash, evicted} => {
                            // window eviction.
                            // TODO: check frequencies, see if we can insert on slru
                            InsertResultShared::Success
                        }
                        res @ _ => {
                            // either success or hash clash. nothing we can do
                            res
                        }
                    }
                }
            }
            Some(old_entry) => {
                    InsertResultShared::Success
            }
        }
    }
}
