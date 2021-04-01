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
use crate::user::EntryT;

/// SLRU ( https://en.wikipedia.org/wiki/Cache_replacement_policies#Segmented_LRU_(SLRU) )
/// is a Segmented LRU it consists of two LRU:
///  * probation LRU: for items that have been just added
///  * protected LRU: items that were in the probation LRU and received a HIT
/// W-TinyLRU specifies an 20-80 split, with 80% for the probation LRU
pub struct SLRU<K, V, Umeta, HB>
where
    Umeta: user::Meta<V>,
{
    _hmap:
        ::std::collections::HashMap<K, user::Entry<K, V, SLRUCid, Umeta>, HB>,
    _slru:
        SLRUShared<user::Entry<K, V, SLRUCid, Umeta>, K, V, SLRUCid, Umeta, HB>,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum SLRUCid {
    Probation,
    Protected,
}

impl<
        K: ::std::hash::Hash + Clone + Eq,
        V,
        Umeta: user::Meta<V>,
        HB: ::std::hash::BuildHasher,
    > SLRU<K, V, Umeta, HB>
{
    pub fn new(
        probation_entries: usize,
        protected_entries: usize,
        extra_hashmap_capacity: usize,
        hash_builder: HB,
    ) -> SLRU<K, V, Umeta, HB> {
        SLRU {
            _hmap: ::std::collections::HashMap::with_capacity_and_hasher(
                1 + probation_entries
                    + protected_entries
                    + extra_hashmap_capacity,
                hash_builder,
            ),
            _slru: SLRUShared::<
                user::Entry<K, V, SLRUCid, Umeta>,
                K,
                V,
                SLRUCid,
                Umeta,
                HB,
            >::new(
                (probation_entries, SLRUCid::Probation),
                (protected_entries, SLRUCid::Protected),
            ),
        }
    }
    pub fn insert(&mut self, key: K, val: V) -> InsertResult<(K, V, Umeta)> {
        self.insert_with_meta(key, val, Umeta::new())
    }
    pub fn insert_with_meta(
        &mut self,
        key: K,
        val: V,
        user_data: Umeta,
    ) -> InsertResult<(K, V, Umeta)> {
        let e = user::Entry::<K, V, SLRUCid, Umeta>::new_entry(
            None,
            None,
            key.clone(),
            val,
            SLRUCid::Probation,
            user_data,
        );
        // insert and get length and a ref to the value just inserted
        // we will use this ref to fix the linked lists in ll_tail/ll_head
        // of the various elements
        let maybe_old_entry = self._hmap.insert(key.clone(), e);
        match self
            ._slru
            .insert_shared(&mut self._hmap, maybe_old_entry, &key)
        {
            InsertResultShared::OldEntry(e) => {
                InsertResult::OldEntry(e.deconstruct())
            }
            InsertResultShared::OldTail(tail) => {
                InsertResult::OldTail(tail.deconstruct())
            }
            InsertResultShared::OldTailKey(tailkey) => {
                let removed = self._hmap.remove(&tailkey).unwrap();
                InsertResult::OldTail(removed.deconstruct())
            }
            InsertResultShared::Success => InsertResult::Success,
        }
    }

    pub fn remove(&mut self, key: &K) -> Option<(V, Umeta)> {
        match self._hmap.remove(key) {
            None => None,
            Some(entry) => {
                self._slru.remove_shared(&entry);
                let (_, val, meta) = entry.deconstruct();
                Some((val, meta))
            }
        }
    }
    pub fn clear(&mut self) {
        self._hmap.clear();
        self._slru.clear_shared()
    }
    // FIXME: we should run the 'on_get' function on the user meta
    pub fn get(&self, key: &K) -> Option<(&V, &Umeta)> {
        match self._hmap.get(key) {
            None => None,
            Some(entry) => Some((entry.get_val(), entry.get_user())),
        }
    }
    pub fn get_mut(&mut self, key: &K) -> Option<(&mut V, &mut Umeta)> {
        match self._hmap.get_mut(key) {
            None => None,
            //Some(mut entry) => Some((entry.get_val(), entry.get_user())),
            Some(entry) => Some(entry.get_val_user_mut()),
        }
    }
}

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
        probation: (usize, Cid),
        protected: (usize, Cid),
    ) -> SLRUShared<E, K, V, Cid, Umeta, HB> {
        SLRUShared {
            _probation: crate::lru::LRUShared::<E, K, V, Cid, Umeta, HB>::new(
                probation.0,
                probation.1,
            ),
            _protected: crate::lru::LRUShared::<E, K, V, Cid, Umeta, HB>::new(
                protected.0,
                protected.1,
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
        if just_inserted.get_cache_id() == self._probation.get_cache_id() {
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
