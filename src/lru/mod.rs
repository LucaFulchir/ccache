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

use crate::results::{Error, InsertResult};
use ::std::collections::HashMap;

struct Entry<K, V> {
    // linked list towards head
    ll_head: Option<*mut Entry<K, V>>,
    // linked list towards tail
    ll_tail: Option<*mut Entry<K, V>>,
    key: K,
    val: V,
}

/// Simple LRU implementation
/// note that we store the value as-is, so **if you need to grow the
/// LRU dynamically, make sure to use `Box<V>` as the value**
// TODO: generalize: K in the first Hashmap template parameter is not
// necessarily the same K in the Entry<K>
// (e.g: could be a pointer to Entry<K>.key)
pub struct LRU<K, V, HB> {
    _capacity: usize,
    _hmap: HashMap<K, Entry<K, V>, HB>,

    _head: Option<*mut Entry<K, V>>,
    _tail: Option<*mut Entry<K, V>>,
}

impl<K: ::std::hash::Hash + Clone + Eq, V, HB: ::std::hash::BuildHasher>
    LRU<K, V, HB>
{
    pub fn new(
        entries: usize,
        extra_hashmap_capacity: usize,
        hash_builder: HB,
    ) -> LRU<K, V, HB> {
        LRU {
            _capacity: entries,
            // due to the possibilities of hash clashing, if the user has chosen
            // a weakish hash function, we can always clash, even if we have
            // a free entry.
            // So on insert we will insert on the hashmap before removing the
            // tail. This can generate a moment where hashmap.len() >
            // capacity. To avoid reallocation, allocate one more
            // entry from the beginning.
            //
            // The extra capacity is to leave breathing room to the hasmap
            // sine I did not check the actual implementation and do not know
            // if having it too full gives problems (TODO: check)
            _hmap: ::std::collections::HashMap::with_capacity_and_hasher(
                1 + entries + extra_hashmap_capacity,
                hash_builder,
            ),
            _head: None,
            _tail: None,
        }
    }
    pub fn insert(&mut self, key: K, val: V) -> InsertResult<K, V> {
        let e = Entry {
            ll_head: None,
            ll_tail: self._head,
            key: key.clone(),
            val: val,
        };
        // insert and get length and a ref to the value just inserted
        // we will use this ref to fix the linked lists in ll_tail/ll_head
        // of the various elements
        let maybe_old_entry = self._hmap.insert(key.clone(), e);
        let hashmap_len = self._hmap.len();
        let just_inserted = self._hmap.get_mut(&key).unwrap();

        match maybe_old_entry {
            Some(old_entry) => {
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
                // we did not clash with anything, but we might still be over
                // capacity
                if hashmap_len >= self._capacity {
                    unsafe {
                        (*self._head.unwrap()).ll_head = Some(just_inserted);
                    }
                    self._head = Some(just_inserted);
                    let removed = self
                        ._hmap
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
        self._hmap.clear();
        self._head = None;
        self._tail = None;
    }
    pub fn remove(&mut self, key: &K) -> Option<V> {
        match self._hmap.remove(key) {
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
    pub fn contains_key(&self, key: &K) -> bool {
        self._hmap.contains_key(key)
    }
    pub fn make_head(&mut self, key: &K) -> Result<(), Error> {
        // A tiny bit quicker than insert
        match self._hmap.get_mut(&key) {
            None => Err(Error::KeyNotFound),
            Some(entry) => {
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
                Ok(())
            }
        }
    }
}
