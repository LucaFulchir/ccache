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

use bitvec::access;

use crate::results::{InsertResult, InsertResultShared};
use crate::user;
use crate::user::EntryT;

#[derive(Clone, Copy, PartialEq, Eq)]
struct SLRUCidNone {}
impl Default for SLRUCidNone {
    fn default() -> Self {
        SLRUCidNone {}
    }
}
#[derive(Clone, Copy, PartialEq, Eq)]
struct SLRUCidProbation {}
impl Default for SLRUCidProbation {
    fn default() -> Self {
        SLRUCidProbation {}
    }
}
#[derive(Clone, Copy, PartialEq, Eq)]
struct SLRUCidProtected {}
impl Default for SLRUCidProtected {
    fn default() -> Self {
        SLRUCidProtected {}
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum SLRUCid {
    None(SLRUCidNone),
    Probation(SLRUCidProbation),
    Protected(SLRUCidProtected),
}
impl Default for SLRUCid {
    fn default() -> Self {
        SLRUCid::None(SLRUCidNone::default())
    }
}

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
    _slru: SLRUShared<
        user::Entry<K, V, SLRUCid, Umeta>,
        K,
        V,
        SLRUCid,
        Umeta,
        fn(::std::ptr::NonNull<user::Entry<K, V, SLRUCid, Umeta>>),
        HB,
    >,
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
                fn(::std::ptr::NonNull<user::Entry<K, V, SLRUCid, Umeta>>),
                HB,
            >::new(
                (
                    probation_entries,
                    SLRUCid::Probation(SLRUCidProbation::default()),
                ),
                (
                    protected_entries,
                    SLRUCid::Protected(SLRUCidProtected::default()),
                ),
                crate::scan::null_scan::<
                    user::Entry<K, V, SLRUCid, Umeta>,
                    K,
                    V,
                    SLRUCid,
                    Umeta,
                >,
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
            SLRUCid::default(),
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
    pub fn get(&mut self, key: &K) -> Option<(&V, &Umeta)> {
        match self._hmap.get_mut(key) {
            None => None,
            Some(entry) => {
                entry.user_on_get();
                Some((entry.get_val(), entry.get_user()))
            }
        }
    }
    pub fn get_mut(&mut self, key: &K) -> Option<(&mut V, &mut Umeta)> {
        match self._hmap.get_mut(key) {
            None => None,
            Some(entry) => {
                entry.user_on_get();
                Some(entry.get_val_user_mut())
            }
        }
    }
}
pub struct SLRUShared<E, K, V, Cid, Umeta, Fscan, HB>
where
    E: user::EntryT<K, V, Cid, Umeta>,
    V: Sized,
    Cid: Eq + Copy + Default,
    Umeta: user::Meta<V>,
    Fscan: Sized + Fn(::std::ptr::NonNull<E>),
{
    _probation: crate::lru::LRUShared<E, K, V, Cid, Umeta, Fscan, HB>,
    _protected: crate::lru::LRUShared<E, K, V, Cid, Umeta, Fscan, HB>,
}

impl<
        E: user::EntryT<K, V, Cid, Umeta>,
        K: ::std::hash::Hash + Clone + Eq,
        V,
        Cid: Eq + Copy + Default,
        Umeta: user::Meta<V>,
        Fscan: Sized + Fn(::std::ptr::NonNull<E>) + Copy,
        HB: ::std::hash::BuildHasher,
    > SLRUShared<E, K, V, Cid, Umeta, Fscan, HB>
{
    pub fn new(
        probation: (usize, Cid),
        protected: (usize, Cid),
        access_scan: Fscan,
    ) -> SLRUShared<E, K, V, Cid, Umeta, Fscan, HB> {
        SLRUShared {
            _probation:
                crate::lru::LRUShared::<E, K, V, Cid, Umeta, Fscan, HB>::new(
                    probation.0,
                    probation.1,
                    access_scan.clone(),
                ),
            _protected:
                crate::lru::LRUShared::<E, K, V, Cid, Umeta, Fscan, HB>::new(
                    protected.0,
                    protected.1,
                    access_scan,
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
            // inserted twice. promote to protected
            // Note that since we already found the key, we can not have had any
            // clash (maybe_old_entry == None)
            self._probation.remove_shared(just_inserted);
            self._protected.insert_shared(hmap, None, key)
        } else if just_inserted.get_cache_id() == self._protected.get_cache_id()
        {
            // inserted more than once, in protected
            // Note that since we already found the key, we can not have had any
            // clash (maybe_old_entry == None)
            self._protected.insert_shared(hmap, None, key)
        } else {
            // new insert, not in any cache.
            // We might have had a clash, but we will insert into probation
            match maybe_old_entry {
                None => self._probation.insert_shared(hmap, None, key),
                Some(old_entry) => {
                    // old_entry might be either in probation or protected.
                    // But the only way for us to have an "old_entry" instead of
                    // None is to have generated a clash on insert.
                    // This means that "just_inserted" is a new key, not an old
                    // one, or we would not have had the clash.
                    if old_entry.get_cache_id()
                        == self._probation.get_cache_id()
                    {
                        // old_entry in probation.
                        self._probation.insert_shared(
                            hmap,
                            Some(old_entry),
                            key,
                        )
                    } else {
                        // old_entry in protected
                        // FIXME: we won't alert the user of the removal of
                        //   old_entry!
                        self._protected.remove_shared(&old_entry.into());
                        self._probation.insert_shared(hmap, None, key)
                    }
                }
            }
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
