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

use crate::results::InsertResult;
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
pub struct LRUShared<E, K, V, Cid, Umeta, HB>
where
    E: user::EntryT<K, V, Cid, Umeta>,
    V: Sized,
    Cid: Eq,
    Umeta: user::Meta<V>, {}
pub struct SLRUShared<K, V, U, HB>
where
    U: user::Meta<V>,
{
    _probation: crate::lru::LRUShared<E, K, V, Cid, Umeta, HB>,
    _protected: crate::lru::LRUShared<E, K, V, Cid, Umeta, HB>,
}

impl<
        K: ::std::hash::Hash + Clone + Eq,
        V,
        U: user::Meta<V>,
        HB: ::std::hash::BuildHasher,
    > SLRUShared<K, V, U, HB>
{
    pub fn new(entries: usize) -> SLRUShared<K, V, U, HB> {
        let mut probation_entries: usize = (entries as f64 * 0.2) as usize;
        if entries > 0 && probation_entries == 0 {
            probation_entries = 1
        }
        SLRUShared {
            _probation: crate::lru::LRUShared::<K, V, U, HB>::new(
                probation_entries,
            ),
            _protected: crate::lru::LRUShared::<K, V, U, HB>::new(
                entries - probation_entries,
            ),
        }
    }
    pub fn insert(
        &mut self,
        hmap: &mut ::std::collections::HashMap<K, user::Entry<K, V, U>, HB>,
        key: K,
        val: V,
    ) -> InsertResult<K, V> {
        match self._probation.remove(hmap, &key) {
            Some(_) => {
                // promote to protected
                match self._protected.insert(hmap, key, val) {
                    InsertResult::Success => InsertResult::Success,
                    InsertResult::OldEntry(k, v) => {
                        InsertResult::OldEntry(k, v)
                    }
                    InsertResult::OldTail(k, v) => {
                        // values evicted from the protected LRU go into the
                        // probation LRU
                        self._probation.insert(hmap, k, v)
                    }
                }
            }
            None => {
                match self._protected.make_head(hmap, &key, val) {
                    Some(value) => {
                        // insert in probation
                        self._probation.insert(hmap, key, value)
                    }
                    None => InsertResult::Success,
                }
            }
        }
    }
    pub fn remove(
        &mut self,
        hmap: &mut ::std::collections::HashMap<K, user::Entry<K, V, U>, HB>,
        key: &K,
    ) -> Option<V> {
        match self._probation.remove(hmap, key) {
            Some(val) => Some(val),
            None => match self._protected.remove(hmap, key) {
                Some(val) => Some(val),
                None => None,
            },
        }
    }
    pub fn clear(&mut self) {
        self._probation.clear();
        self._protected.clear();
    }
    pub fn get<'a>(
        &mut self,
        hmap: &'a mut ::std::collections::HashMap<K, user::Entry<K, V, U>, HB>,
        key: &K,
    ) -> Option<(&'a V, &'a U)> {
        // note that we share the hmap, so we don't need to check both probation
        // and protected
        self._probation.get(hmap, key)
    }
    pub fn get_mut<'a>(
        &mut self,
        hmap: &'a mut ::std::collections::HashMap<K, user::Entry<K, V, U>, HB>,
        key: &K,
    ) -> Option<(&'a mut V, &'a mut U)> {
        // note that we share the hmap, so we don't need to check both probation
        // and protected
        self._probation.get_mut(hmap, key)
    }
}
