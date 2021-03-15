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
use crate::UserMeta;

/// SLRU ( https://en.wikipedia.org/wiki/Cache_replacement_policies#Segmented_LRU_(SLRU) )
/// is a Segmented LRU it consists of two LRU:
///  * probation LRU: for items that have been just added
///  * protected LRU: items that were in the probation LRU and received a HIT
/// W-TinyLRU specifies an 20-80 split, with 80% for the probation LRU
pub struct SLRU<K, V, U, HB>
where
    U: UserMeta<V>,
{
    _probation: crate::lru::LRU<K, V, U, HB>,
    _protected: crate::lru::LRU<K, V, U, HB>,
}

impl<
        K: ::std::hash::Hash + Clone + Eq,
        V,
        U: UserMeta<V>,
        HB: ::std::hash::BuildHasher,
    > SLRU<K, V, U, HB>
{
    pub fn new(
        entries: usize,
        extra_hashmap_capacity: usize,
        hash_builder_probation: HB,
        hash_builder_protected: HB,
    ) -> SLRU<K, V, U, HB> {
        let mut probation_entries: usize = (entries as f64 * 0.2) as usize;
        if entries > 0 && probation_entries == 0 {
            probation_entries = 1
        }
        let extra_hashmap_probation: usize = extra_hashmap_capacity / 2;
        SLRU {
            _probation: crate::lru::LRU::new(
                probation_entries,
                extra_hashmap_probation,
                hash_builder_probation,
            ),
            _protected: crate::lru::LRU::new(
                entries - probation_entries,
                extra_hashmap_capacity - extra_hashmap_probation,
                hash_builder_protected,
            ),
        }
    }
    pub fn insert(&mut self, key: K, val: V) -> InsertResult<K, V> {
        match self._probation.remove(&key) {
            Some(_) => {
                // promote to protected
                match self._protected.insert(key, val) {
                    InsertResult::Success => InsertResult::Success,
                    InsertResult::OldEntry(k, v) => {
                        InsertResult::OldEntry(k, v)
                    }
                    InsertResult::OldTail(k, v) => {
                        // values evicted from the protected LRU go into the
                        // probation LRU
                        self._probation.insert(k, v)
                    }
                }
            }
            None => {
                match self._protected.make_head(&key, val) {
                    Some(value) => {
                        // insert in probation
                        self._probation.insert(key, value)
                    }
                    None => InsertResult::Success,
                }
            }
        }
    }
    pub fn remove(&mut self, key: &K) -> Option<V> {
        match self._probation.remove(key) {
            Some(val) => Some(val),
            None => match self._protected.remove(key) {
                Some(val) => Some(val),
                None => None,
            },
        }
    }
    pub fn clear(&mut self) {
        self._probation.clear();
        self._protected.clear();
    }
    pub fn get(&mut self, key: &K) -> Option<(&V, &U)> {
        match self._probation.get(key) {
            Some((v, u)) => Some((v, u)),
            None => self._protected.get(key),
        }
    }
    pub fn get_mut(&mut self, key: &K) -> Option<(&mut V, &mut U)> {
        match self._probation.get_mut(key) {
            Some((v, u)) => Some((v, u)),
            None => self._protected.get_mut(key),
        }
    }
}
