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
pub struct LRU<K, V, U, HB>
where
    V: Sized,
    U: user::Meta<V>,
{
    _hmap: ::std::collections::HashMap<K, user::Entry<K, V, U>, HB>,
    _lru: LRUShared<K, V, U, HB>,
}

impl<
        K: ::std::hash::Hash + Clone + Eq,
        V,
        U: user::Meta<V>,
        HB: ::std::hash::BuildHasher,
    > LRU<K, V, U, HB>
{
    pub fn new(
        entries: usize,
        extra_hashmap_capacity: usize,
        hash_builder: HB,
    ) -> LRU<K, V, U, HB> {
        LRU {
            _hmap: ::std::collections::HashMap::with_capacity_and_hasher(
                1 + entries + extra_hashmap_capacity,
                hash_builder,
            ),
            _lru: LRUShared::<K, V, U, HB>::new(entries),
        }
    }
    pub fn insert(&mut self, key: K, val: V) -> InsertResult<K, V> {
        self._lru.insert(&mut self._hmap, key, val)
    }
    pub fn insert_with_meta(
        &mut self,
        key: K,
        val: V,
        user_data: U,
    ) -> InsertResult<K, V> {
        self._lru
            .insert_with_meta(&mut self._hmap, key, val, user_data)
    }
    pub fn clear(&mut self) {
        self._hmap.clear();
        self._lru.clear()
    }
    pub fn remove(&mut self, key: &K) -> Option<V> {
        self._lru.remove(&mut self._hmap, key)
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

pub struct LRUShared<K, V, U, HB>
where
    V: Sized,
    U: user::Meta<V>,
{
    _capacity: usize,

    _head: Option<*mut user::Entry<K, V, U>>,
    _tail: Option<*mut user::Entry<K, V, U>>,
    _hashbuilder: ::std::marker::PhantomData<HB>,
}

impl<
        K: ::std::hash::Hash + Clone + Eq,
        V,
        U: user::Meta<V>,
        HB: ::std::hash::BuildHasher,
    > LRUShared<K, V, U, HB>
{
    pub fn new(entries: usize) -> LRUShared<K, V, U, HB> {
        LRUShared {
            _capacity: entries,
            _head: None,
            _tail: None,
            _hashbuilder: std::marker::PhantomData,
        }
    }
    pub fn insert(
        &mut self,
        hmap: &mut ::std::collections::HashMap<K, user::Entry<K, V, U>, HB>,
        key: K,
        val: V,
    ) -> InsertResult<K, V> {
        self.insert_with_meta(hmap, key, val, U::new())
    }
    pub fn insert_with_meta(
        &mut self,
        hmap: &mut ::std::collections::HashMap<K, user::Entry<K, V, U>, HB>,
        key: K,
        val: V,
        user_data: U,
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
        let hashmap_len = hmap.len();
        let just_inserted = hmap.get_mut(&key).unwrap();

        match maybe_old_entry {
            Some(mut old_entry) => {
                just_inserted.user_data.on_insert(
                    Some(&old_entry.user_data),
                    Some(&mut old_entry.val),
                );
                // we removed something that was in the linked
                // list due to hash clashing. fix the linked lists.
                // In this case even the head or tail
                // might need to be changed
                if None == old_entry.ll_head {
                    // we removed the old head with the hash clash
                    just_inserted.ll_tail = old_entry.ll_tail;
                    match old_entry.ll_tail {
                        None => {
                            // None    == old_entry.ll_head
                            // None    == old_entry.ll_tail
                            // basically the only element in the LRU
                            // noth head and tail
                            self._tail = Some(just_inserted);
                        }
                        Some(node) => unsafe {
                            // None    == old_entry.ll_head
                            // Some(_) == old_entry.ll_tail
                            // the head of the LRU
                            (*node).ll_head = Some(just_inserted)
                        },
                    }
                    self._head = Some(just_inserted);
                    return InsertResult::OldEntry(
                        old_entry.key,
                        old_entry.val,
                    );
                } else {
                    if None == old_entry.ll_tail {
                        // Some(_) == old_entry.ll_head
                        // None    == old_entry.ll_tail
                        // we removed the old tail, with a hash clash
                        // the new tail is the prev of the old tail.
                        let node = old_entry.ll_head.unwrap();
                        unsafe {
                            (*node).ll_tail = None;
                            (*self._head.unwrap()).ll_head =
                                Some(just_inserted);
                        }
                        self._head = Some(just_inserted);
                        self._tail = Some(node);
                        return InsertResult::OldTail(
                            old_entry.key,
                            old_entry.val,
                        );
                    } else {
                        // Some(_) == old_entry.ll_head
                        // Some(_) == old_entry.ll_tail
                        // we removed something in the middle
                        unsafe {
                            (*old_entry.ll_tail.unwrap()).ll_head =
                                old_entry.ll_head;
                            (*old_entry.ll_head.unwrap()).ll_tail =
                                old_entry.ll_tail;
                            (*self._head.unwrap()).ll_head =
                                Some(just_inserted);
                        }
                        self._head = Some(just_inserted);
                        return InsertResult::OldEntry(
                            old_entry.key,
                            old_entry.val,
                        );
                    }
                }
            }
            None => {
                just_inserted.user_data.on_insert(None, None);
                // we did not clash with anything, but we might still be over
                // capacity
                if hashmap_len >= self._capacity {
                    unsafe {
                        (*self._head.unwrap()).ll_head = Some(just_inserted);
                    }
                    self._head = Some(just_inserted);
                    let removed = hmap
                        .remove(unsafe { &(*self._tail.unwrap()).key })
                        .unwrap();
                    unsafe {
                        let node = (*self._tail.unwrap()).ll_head.unwrap();
                        (*node).ll_tail = None;
                        self._tail = Some(node);
                    }
                    return InsertResult::OldTail(removed.key, removed.val);
                }
                match self._head {
                    None => {
                        // first entry in the LRU, both head and tail
                        self._head = Some(just_inserted);
                        self._tail = Some(just_inserted);
                    }
                    Some(node) => {
                        // just a new entry on a non-filled LRU

                        unsafe { (*node).ll_head = Some(just_inserted) };
                        self._head = Some(just_inserted);
                    }
                }
                return InsertResult::Success;
            }
        }
    }
    pub fn clear(&mut self) {
        self._head = None;
        self._tail = None;
    }
    pub fn remove(
        &mut self,
        hmap: &mut ::std::collections::HashMap<K, user::Entry<K, V, U>, HB>,
        key: &K,
    ) -> Option<V> {
        match hmap.remove(key) {
            None => None,
            Some(node) => {
                if None == node.ll_head {
                    // we removed the head
                    match node.ll_tail {
                        None => {
                            // None == node.ll_head
                            // None == node.ll_tail
                            // we had only one element, we removed it
                            self._head = None;
                            self._tail = None;
                        }
                        Some(node_tail) => {
                            // None == node.ll_head
                            // Some(_) == node.ll_tail
                            // we removed the head
                            unsafe {
                                (*node_tail).ll_head = None;
                            }
                            self._head = Some(node_tail);
                        }
                    }
                } else {
                    match node.ll_tail {
                        None => {
                            // Some(_) == node.ll_head
                            // None == node.ll_tail
                            // we removed the tail
                            unsafe {
                                (*node.ll_head.unwrap()).ll_tail = None;
                            }
                            self._tail = node.ll_head;
                        }
                        Some(node_tail) => {
                            // Some(_) == node.ll_head
                            // Some(_) == node.ll_tail
                            // we removed an intermediate node
                            unsafe {
                                (*node.ll_head.unwrap()).ll_tail = node.ll_tail;
                                (*node_tail).ll_head = node.ll_head;
                            }
                        }
                    }
                }
                Some(node.val)
            }
        }
    }
    pub fn contains_key(
        &self,
        hmap: &::std::collections::HashMap<K, user::Entry<K, V, U>, HB>,
        key: &K,
    ) -> bool {
        hmap.contains_key(key)
    }
    /// make a key head, while chaning the contents of its value
    /// Returns the value if the key is not present
    pub fn make_head(
        &mut self,
        hmap: &mut ::std::collections::HashMap<K, user::Entry<K, V, U>, HB>,
        key: &K,
        val: V,
    ) -> Option<V> {
        // A tiny bit quicker than insert
        match hmap.get_mut(&key) {
            None => Some(val),
            Some(entry) => {
                entry.val = val;
                match entry.ll_head {
                    None => {
                        // already the head, nothing to do
                    }
                    Some(entry_head) => {
                        unsafe {
                            (*entry_head).ll_tail = entry.ll_tail;
                        }
                        match entry.ll_tail {
                            None => {
                                // we moved the tail to the head.
                                unsafe {
                                    (*self._head.unwrap()).ll_head =
                                        Some(entry);
                                    (*entry).ll_tail = self._head;
                                }
                                self._head = Some(entry);
                                self._tail = Some(entry_head);
                            }
                            Some(entry_tail) => {
                                // we promoted to head something in the middle
                                // of the linked list
                                unsafe {
                                    (*entry_tail).ll_head = Some(entry_head);
                                    (*entry_head).ll_tail = Some(entry_tail);
                                    (*self._head.unwrap()).ll_head =
                                        Some(entry);
                                }
                                self._head = Some(entry);
                            }
                        }
                    }
                }
                None
            }
        }
    }
    pub fn get<'a>(
        &mut self,
        hmap: &'a mut HashMap<K, user::Entry<K, V, U>, HB>,
        key: &K,
    ) -> Option<(&'a V, &'a U)> {
        match hmap.get_mut(key) {
            None => None,
            Some(entry) => {
                entry.user_data.on_get(&mut entry.val);
                Some((&entry.val, &entry.user_data))
            }
        }
    }
    pub fn get_mut<'a>(
        &mut self,
        hmap: &'a mut HashMap<K, user::Entry<K, V, U>, HB>,
        key: &K,
    ) -> Option<(&'a mut V, &'a mut U)> {
        match hmap.get_mut(key) {
            None => None,
            Some(entry) => {
                entry.user_data.on_get(&mut entry.val);
                Some((&mut entry.val, &mut entry.user_data))
            }
        }
    }
}
