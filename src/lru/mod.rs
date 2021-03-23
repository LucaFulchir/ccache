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
use ::std::collections::HashMap;

// Simple LRU implementation
/// note that we store the value as-is, so **if you need to grow the
/// LRU dynamically, make sure to use `Box<V>` as the value**
// TODO: generalize: K in the first Hashmap template parameter is not
// necessarily the same K in the user::Entry<K>
// (e.g: could be a pointer to user::Entry<K>.key)
/*
pub struct LRU<K, V, Umeta, HB>
where
    V: Sized,
    U: user::Meta<V>,
{
    _hmap: ::std::collections::HashMap<
        K,
        user::Etry<K, V, ::std::marker::PhantomData<K>, Umeta>,
        HB,
    >,
    _lru: LRUShared<K, V, ::std::marker::PhantomData<K>, Umeta, HB>,
}

impl<
        K: ::std::hash::Hash + Clone + Eq,
        V,
        Umeta: user::Meta<V>,
        HB: ::std::hash::BuildHasher,
    > LRU<K, V, Umeta, HB>
{
    pub fn new(
        entries: usize,
        extra_hashmap_capacity: usize,
        hash_builder: HB,
    ) -> LRU<K, V, Umeta, HB> {
        LRU {
            _hmap: ::std::collections::HashMap::with_capacity_and_hasher(
                1 + entries + extra_hashmap_capacity,
                hash_builder,
            ),
            _lru:
                LRUShared::<K, V, ::std::marker::PhantomData<K>, Umeta, HB>::new(
                    entries,
                ),
        }
    }
    pub fn insert(&mut self, key: K, val: V) -> InsertResult<K, V> {
        self.insert_with_meta(key, val, Umeta::new())
    }
    pub fn insert_with_meta(
        &mut self,
        key: K,
        val: V,
        user_data: Umeta,
    ) -> InsertResult<K, V> {
        let e = user::Entry {
            ll_head: None,
            ll_tail: self._head,
            key: key.clone(),
            val: val,
            user_data: user_data,
        };
        // insert and get length and a ref to the value just inserted
        // we will use this ref to fix the linked lists in ll_tail/ll_head
        // of the various elements
        let maybe_old_entry = hmap.insert(key.clone(), e);
        let just_inserted = hmap.get_mut(&key).unwrap();
        self._lru.insert_with_meta(
            &mut self._hmap,
            &maybe_old_entry,
            &mut just_inserted,
        )
    }
    pub fn clear(&mut self) {
        self._hmap.clear();
        self._lru.clear()
    }
    pub fn remove(&mut self, key: &K) -> Option<V> {
        let entry = self._hmap.remove(key);
        self._lru.remove(&mut entry);
    }
    pub fn contains_key(&self, key: &K) -> bool {
        self._lru.contains_key(&self._hmap, key)
    }
    pub fn make_head(&mut self, key: &K, val: V) -> Option<V> {
        self._lru.make_head(&mut self._hmap, key, val)
    }
    pub fn get(&mut self, key: &K) -> Option<(&V, &U)> {
        self._lru.get(&mut self._hmap, key)
    }
    pub fn get_mut(&mut self, key: &K) -> Option<(&mut V, &mut U)> {
        self._lru.get_mut(&mut self._hmap, key)
    }
}
*/
pub struct LRUShared<E, K, V, Cid, Umeta, HB>
where
    E: user::EntryT<K, V, Cid, Umeta>,
    V: Sized,
    Cid: ::num_traits::int::PrimInt,
    Umeta: user::Meta<V>,
{
    _capacity: usize,
    _used: usize,

    _head: Option<*mut E>,
    _tail: Option<*mut E>,
    _cache_id: Cid,
    _key: ::std::marker::PhantomData<K>,
    _val: ::std::marker::PhantomData<V>,
    _meta: ::std::marker::PhantomData<Umeta>,
    _hashbuilder: ::std::marker::PhantomData<HB>,
}

impl<
        E: user::EntryT<K, V, Cid, Umeta>,
        K: ::std::hash::Hash + Clone + Eq,
        V,
        Cid: ::num_traits::int::PrimInt,
        Umeta: user::Meta<V>,
        HB: ::std::hash::BuildHasher,
    > LRUShared<E, K, V, Cid, Umeta, HB>
{
    /// Build a LRU that works on someone else's hasmap
    /// In this case each cache should have a different `Cid` (Cache ID) so that
    /// everyone known whose elements is being used, and call the proper
    /// cache methods
    pub fn new(
        entries: usize,
        cache_id: Cid,
    ) -> LRUShared<E, K, V, Cid, Umeta, HB> {
        LRUShared {
            _capacity: entries,
            _used: 0,
            _head: None,
            _tail: None,
            _cache_id: cache_id,
            _key: ::std::marker::PhantomData,
            _val: ::std::marker::PhantomData,
            _meta: ::std::marker::PhantomData,
            _hashbuilder: std::marker::PhantomData,
        }
    }
    /// `insert_shared` does not actually insert anything. It will only fix
    /// the LRU linked lists after something has been inserted by the parent
    /// note that ~maybe_old_entry` should be != `None` if and only if the
    /// ols entry is part of the same cache
    pub fn insert_shared(
        &mut self,
        hmap: &mut ::std::collections::HashMap<K, E, HB>,
        maybe_old_entry: Option<E>,
        just_inserted: &mut E,
    ) -> InsertResult<E> {
        self._used += 1;

        match maybe_old_entry {
            None => {
                just_inserted.user_on_insert(None);
                // we did not clash with anything, but we might still be over
                // capacity
                if self._used >= self._capacity {
                    // reset head & tail to correct values, returen old tail
                    unsafe {
                        (*self._head.unwrap())
                            .set_head_ptr(Some(just_inserted));
                    }
                    self._head = Some(just_inserted);
                    let removed = hmap
                        .remove(unsafe { (*self._tail.unwrap()).get_key() })
                        .unwrap();
                    unsafe {
                        let rm_tail_head = removed.get_head_ptr().unwrap();
                        (*rm_tail_head).set_tail_ptr(None);
                        self._tail = Some(rm_tail_head);
                    }
                    return InsertResult::OldTail(removed);
                }
                match self._head {
                    None => {
                        // first entry in the LRU, both head and tail
                        self._head = Some(just_inserted);
                        self._tail = Some(just_inserted);
                    }
                    Some(old_head) => {
                        // just a new entry on a non-filled LRU
                        unsafe {
                            (*old_head).set_head_ptr(Some(just_inserted))
                        };
                        self._head = Some(just_inserted);
                    }
                }
                return InsertResult::Success;
            }
            Some(mut old_entry) => {
                // the callee has added an element to the hashmap, but it
                // clashed with something. We'll have to keep track of it and
                // we should fix it
                //
                // By definition, we know that we get the 'old_entry' only if it
                // is in the same cache_id as us

                just_inserted.user_on_insert(Some(&mut old_entry));
                // the clash was on something in our own cache, we got
                // lucky. In this case even the head or
                // tail might need to be changed
                match old_entry.get_head_ptr() {
                    None => {
                        // we removed the old head with the hash clash
                        just_inserted.set_tail_ptr(old_entry.get_tail_ptr());
                        match old_entry.get_tail_ptr() {
                            None => {
                                // None    == old_entry.head
                                // None    == old_entry.tail
                                // basically the only element in the LRU
                                // both head and tail
                                self._tail = Some(just_inserted);
                            }
                            Some(old_entry_tail) => {
                                unsafe {
                                    // None    == old_entry.head
                                    // Some(_) == old_entry.tail
                                    // the head of the LRU
                                    (*old_entry_tail)
                                        .set_head_ptr(Some(just_inserted));
                                }
                            }
                        }
                        self._head = Some(just_inserted);
                        return InsertResult::OldEntry(old_entry);
                    }
                    Some(old_hentry_head) => {
                        match old_entry.get_tail_ptr() {
                            None => {
                                // Some(_) == old_entry.head
                                // None    == old_entry.tail
                                // we removed the old tail, with a hash clash
                                // the new tail is the prev of the old tail.
                                let old_entry_head =
                                    old_entry.get_head_ptr().unwrap();
                                unsafe {
                                    (*old_entry_head).set_tail_ptr(None);
                                    (*self._head.unwrap())
                                        .set_head_ptr(Some(just_inserted));
                                }
                                self._head = Some(just_inserted);
                                self._tail = Some(old_entry_head);
                                return InsertResult::OldTail(old_entry);
                            }
                            Some(old_entry_tail) => {
                                // Some(_) == old_entry.head
                                // Some(_) == old_entry.tail
                                // we removed something in the middle
                                unsafe {
                                    (*old_entry.get_tail_ptr().unwrap())
                                        .set_head_ptr(old_entry.get_head_ptr());
                                    (*old_entry.get_head_ptr().unwrap())
                                        .set_tail_ptr(old_entry.get_tail_ptr());
                                    (*self._head.unwrap())
                                        .set_head_ptr(Some(just_inserted));
                                }
                                self._head = Some(just_inserted);
                                return InsertResult::OldEntry(old_entry);
                            }
                        }
                    }
                }
            }
        }
    }
    pub fn clear_shared(&mut self) {
        self._head = None;
        self._tail = None;
    }
    pub fn remove_shared(&mut self, entry: &mut E) {
        if None == entry.get_head_ptr() {
            // we removed the head
            match entry.get_tail_ptr() {
                None => {
                    // None == entry.head
                    // None == entry.tail
                    // we had only one element, we removed it
                    self._head = None;
                    self._tail = None;
                }
                Some(entry_tail) => {
                    // None == entry.head
                    // Some(_) == entry.tail
                    // we removed the head
                    unsafe {
                        (*entry_tail).set_head_ptr(None);
                    }
                    self._head = Some(entry_tail);
                }
            }
        } else {
            match entry.get_tail_ptr() {
                None => {
                    // Some(_) == entry.head
                    // None == entry.tail
                    // we removed the tail
                    unsafe {
                        (*entry.get_head_ptr().unwrap()).set_tail_ptr(None);
                    }
                    self._tail = entry.get_head_ptr();
                }
                Some(entry_tail) => {
                    // Some(_) == entry.head
                    // Some(_) == entry.tail
                    // we removed an intermediate entry
                    unsafe {
                        (*entry_tail).set_head_ptr(entry.get_head_ptr());
                        (*entry.get_head_ptr().unwrap())
                            .set_tail_ptr(entry.get_tail_ptr());
                    }
                }
            }
        }
    }
    /// make the key the head of the LRU.
    pub fn make_head(&mut self, entry: &mut E) {
        match entry.get_head_ptr() {
            None => {
                // already the head, nothing to do
            }
            Some(entry_head) => {
                unsafe {
                    (*entry_head).set_tail_ptr(entry.get_tail_ptr());
                }
                match entry.get_tail_ptr() {
                    None => {
                        // we moved the tail to the head.
                        unsafe {
                            (*self._head.unwrap()).set_head_ptr(Some(entry));
                            entry.set_tail_ptr(self._head);
                        }
                        self._head = Some(entry);
                        self._tail = Some(entry_head);
                    }
                    Some(entry_tail) => {
                        // we promoted to head something in the middle
                        // of the linked list
                        unsafe {
                            (*entry_tail).set_head_ptr(Some(entry_head));
                            (*entry_head).set_tail_ptr(Some(entry_tail));
                            (*self._head.unwrap()).set_head_ptr(Some(entry));
                        }
                        self._head = Some(entry);
                    }
                }
            }
        }
    }
}
