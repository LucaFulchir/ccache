/*
 * Copyright 2021 Luca Fulchir <luca@fenrirproject.org>
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

use crate::results::{InsertResult, InsertResultShared};
use crate::user;

/// SLRU ( https://en.wikipedia.org/wiki/Cache_replacement_policies#Segmented_LRU_(SLRU) )
/// is a Segmented LRU it consists of two LRU:
///  * probation LRU: for items that have been just added
///  * protected LRU: items that were in the probation LRU and received a HIT
/// W-TinyLRU specifies an 20-80 split, with 80% for the probation LRU
/*
pub struct SLRU<K, V, U, HB>
where
    U: user::Meta<V>,
{
    _hmap: ::std::collections::HashMap<K, user::Entry<K, V, U>, HB>,
    _slru: SLRUShared<K, V, U, HB>,
}

impl<
        K: ::std::hash::Hash + Clone + Eq,
        V,
        U: user::Meta<V>,
        HB: ::std::hash::BuildHasher,
    > SLRU<K, V, U, HB>
{
    pub fn new(
        entries: usize,
        extra_hashmap_capacity: usize,
        hash_builder: HB,
    ) -> SLRU<K, V, U, HB> {
        let mut probation_entries: usize = (entries as f64 * 0.2) as usize;
        if entries > 0 && probation_entries == 0 {
            probation_entries = 1
        }
        let extra_hashmap_probation: usize = extra_hashmap_capacity / 2;
        SLRU {
            _hmap: ::std::collections::HashMap::with_capacity_and_hasher(
                1 + entries + extra_hashmap_capacity,
                hash_builder,
            ),
            _slru: SLRUShared::<K, V, U, HB>::new(entries),
        }
    }
    pub fn insert(&mut self, key: K, val: V) -> InsertResult<K, V> {
        self._slru.insert(&mut self._hmap, key, val)
    }
    pub fn remove(&mut self, key: &K) -> Option<V> {
        self._slru.remove(&mut self._hmap, key)
    }
    pub fn clear(&mut self) {
        self._hmap.clear();
        self._slru.clear()
    }
    pub fn get(&mut self, key: &K) -> Option<(&V, &U)> {
        self._slru.get(&mut self._hmap, key)
    }
    pub fn get_mut(&mut self, key: &K) -> Option<(&mut V, &mut U)> {
        self._slru.get_mut(&mut self._hmap, key)
    }
}
*/
pub struct SLRUShared<E, K, V, Cid, Umeta, HB>
where
    E: user::EntryT<K, V, Cid, Umeta>,
    V: Sized,
    Cid: Eq + Copy,
    Umeta: user::Meta<V>,
{
    _probation: crate::lru::LRUShared<E, K, V, Cid, Umeta, HB>,
    _protected: crate::lru::LRUShared<E, K, V, Cid, Umeta, HB>,
}

impl<
        E: user::EntryT<K, V, Cid, Umeta>,
        K: ::std::hash::Hash + Clone + Eq,
        V,
        Cid: Eq + Copy,
        Umeta: user::Meta<V>,
        HB: ::std::hash::BuildHasher,
    > SLRUShared<E, K, V, Cid, Umeta, HB>
{
    pub fn new(
        probation_entries: usize,
        protected_entries: usize,
        probation_cache_id: Cid,
        protected_cache_id: Cid,
    ) -> SLRUShared<E, K, V, Cid, Umeta, HB> {
        SLRUShared {
            _probation: crate::lru::LRUShared::<E, K, V, Cid, Umeta, HB>::new(
                probation_entries,
                probation_cache_id,
            ),
            _protected: crate::lru::LRUShared::<E, K, V, Cid, Umeta, HB>::new(
                protected_entries,
                protected_cache_id,
            ),
        }
    }
    pub fn insert_shared(
        &mut self,
        hmap: &mut ::std::collections::HashMap<K, E, HB>,
        maybe_old_entry: Option<E>,
        key: &K,
    ) -> InsertResultShared<E, K> {
        let just_inserted = hmap.get_mut(&key).unwrap();
        if *just_inserted.get_cache_id_mut() == self._probation.get_cache_id() {
            self._probation.remove_shared(just_inserted);
            // promote it to protected
            match self._protected.insert_shared(hmap, maybe_old_entry, key) {
                InsertResultShared::OldTailKey(oldkey) => {
                    // out of protected, into probation
                    self._probation.insert_shared(hmap, None, &oldkey)
                }
                res @ _ => {
                    // Either there was a hash clash (returned OldEntry/OldTail)
                    // or Succecss. any case, just return it, nothing to do
                    res
                }
            }
        } else {
            // put in probation
            self._probation.insert_shared(hmap, maybe_old_entry, key)
        }
    }
    pub fn clear_shared(&mut self) {
        self._probation.clear_shared();
        self._protected.clear_shared();
    }
    pub fn remove_shared(&mut self, entry: &E) {
        if entry.get_cache_id() == self._probation.get_cache_id() {
            self._probation.remove_shared(entry)
        } else {
            self._protected.remove_shared(entry)
        }
    }
    pub fn get_cache_ids(&self) -> (Cid, Cid) {
        (
            self._probation.get_cache_id(),
            self._protected.get_cache_id(),
        )
    }
}
