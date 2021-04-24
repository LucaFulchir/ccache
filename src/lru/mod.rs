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

/// LRU implementation that wraps LRUShared
/// note that we store the value as-is and we have pointers to those,
/// so **if you need to grow the LRU dynamically, make sure to use `Box<V>
/// as the value**
// TODO: generalize: K in the first Hashmap template parameter is not
// necessarily the same K in the user::Entry<K>
// (e.g: could be a pointer to user::Entry<K>.key)
type LRUEntry<K, V, Umeta> =
    user::Entry<K, V, ::std::marker::PhantomData<K>, Umeta>;
pub struct LRU<K, V, Umeta, HB>
where
    V: Sized,
    Umeta: user::Meta<V>,
{
    _hmap: ::std::collections::HashMap<K, LRUEntry<K, V, Umeta>, HB>,
    _lru: LRUShared<
        LRUEntry<K, V, Umeta>,
        K,
        V,
        ::std::marker::PhantomData<K>,
        Umeta,
        fn(::std::ptr::NonNull<LRUEntry<K, V, Umeta>>),
        HB,
    >,
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
            _lru: LRUShared::<
                LRUEntry<K, V, Umeta>,
                K,
                V,
                ::std::marker::PhantomData<K>,
                Umeta,
                fn(::std::ptr::NonNull<LRUEntry<K, V, Umeta>>),
                HB,
            >::new(
                entries,
                ::std::marker::PhantomData,
                crate::scan::null_scan::<
                    LRUEntry<K, V, Umeta>,
                    K,
                    V,
                    ::std::marker::PhantomData<K>,
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
        let e =
            user::Entry::<K, V, ::std::marker::PhantomData<K>, Umeta>::new_entry(
                None,
                None,
                key.clone(),
                val,
                ::std::marker::PhantomData,
                user_data,
            );
        // insert and get length and a ref to the value just inserted
        // we will use this ref to fix the linked lists in ll_tail/ll_head
        // of the various elements
        let maybe_old_entry = self._hmap.insert(key.clone(), e);
        match self
            ._lru
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
    pub fn clear(&mut self) {
        self._hmap.clear();
        self._lru.clear_shared()
    }
    pub fn remove(&mut self, key: &K) -> Option<(V, Umeta)> {
        match self._hmap.remove(key) {
            None => None,
            Some(entry) => {
                self._lru.remove_shared(&entry);
                let (_, val, meta) = entry.deconstruct();
                Some((val, meta))
            }
        }
    }
    pub fn contains_key(&self, key: &K) -> bool {
        self._hmap.contains_key(&key)
    }
    /// If present, make the entry the head of the LRU, and return pointers to
    /// the values
    pub fn make_head(&mut self, key: &K) -> Option<(&V, &Umeta)> {
        match self._hmap.get_mut(key) {
            None => None,
            Some(mut entry) => {
                self._lru.make_head(&mut entry);
                Some((entry.get_val(), entry.get_user()))
            }
        }
    }
    pub fn get(&mut self, key: &K) -> Option<(&V, &Umeta)> {
        match self._hmap.get_mut(key) {
            None => None,
            Some(mut entry) => {
                self._lru.on_get(&mut entry);
                Some((entry.get_val(), entry.get_user()))
            }
        }
    }
    pub fn get_mut(&mut self, key: &K) -> Option<(&mut V, &mut Umeta)> {
        match self._hmap.get_mut(key) {
            None => None,
            //Some(mut entry) => Some((entry.get_val(), entry.get_user())),
            Some(mut entry) => {
                self._lru.on_get(&mut entry);
                Some(entry.get_val_user_mut())
            }
        }
    }
}
pub struct LRUShared<E, K, V, Cid, Umeta, Fscan, HB>
where
    E: user::EntryT<K, V, Cid, Umeta>,
    V: Sized,
    Cid: crate::cid::Cid,
    Fscan: Sized + Fn(::std::ptr::NonNull<E>),
    Umeta: user::Meta<V>,
{
    _capacity: usize,
    _used: usize,

    _head: Option<::std::ptr::NonNull<E>>,
    _tail: Option<::std::ptr::NonNull<E>>,
    _cache_id: Cid,
    _key: ::std::marker::PhantomData<K>,
    _val: ::std::marker::PhantomData<V>,
    _meta: ::std::marker::PhantomData<Umeta>,
    _hashbuilder: ::std::marker::PhantomData<HB>,
    _scan: crate::scan::Scan<E, K, V, Cid, Umeta, Fscan>,
}

impl<
        E: user::EntryT<K, V, Cid, Umeta>,
        K: ::std::hash::Hash + Clone + Eq,
        V,
        Cid: crate::cid::Cid,
        Umeta: user::Meta<V>,
        Fscan: Fn(::std::ptr::NonNull<E>),
        HB: ::std::hash::BuildHasher,
    > LRUShared<E, K, V, Cid, Umeta, Fscan, HB>
{
    /// Build a LRU that works on someone else's hasmap
    /// In this case each cache should have a different `Cid` (Cache ID) so that
    /// everyone known whose elements is being used, and call the proper
    /// cache methods
    pub fn new(
        entries: usize,
        cache_id: Cid,
        access_scan: Fscan,
    ) -> LRUShared<E, K, V, Cid, Umeta, Fscan, HB> {
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
            _scan: crate::scan::Scan::new(access_scan),
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
        key: &K,
    ) -> InsertResultShared<E, K> {
        let just_inserted = hmap.get_mut(&key).unwrap();
        self._used += 1;
        self._scan.apply_raw(just_inserted.into());
        *just_inserted.get_cache_id_mut() = self._cache_id;

        match maybe_old_entry {
            None => {
                just_inserted.user_on_insert(None);
                // we did not clash with anything, but we might still be over
                // capacity
                if self._used >= self._capacity {
                    // reset head & tail to correct values, returen old tail
                    unsafe {
                        self._head
                            .unwrap()
                            .as_mut()
                            .set_head_ptr(Some(just_inserted.into()));
                    }
                    self._head = Some(just_inserted.into());
                    let mut to_remove = self._tail.unwrap();
                    self._scan.check_and_next(to_remove);
                    self._scan.apply_next();
                    unsafe {
                        let mut to_rm_head =
                            to_remove.as_mut().get_head_ptr().unwrap();
                        to_rm_head.as_mut().set_tail_ptr(None);
                        self._tail = Some(to_rm_head);
                        return InsertResultShared::OldTailKey(
                            to_remove.as_mut().get_key().clone(),
                        );
                    }
                }
                match self._head {
                    None => {
                        // first entry in the LRU, both head and tail
                        // since it's the first entry, we don't need to start
                        // scanning anything
                        self._head = Some(just_inserted.into());
                        self._tail = Some(just_inserted.into());
                    }
                    Some(mut old_head) => {
                        // just a new entry on a non-filled LRU
                        unsafe {
                            old_head
                                .as_mut()
                                .set_head_ptr(Some(just_inserted.into()))
                        };
                        self._head = Some(just_inserted.into());
                        self._scan.apply_next();
                    }
                }
                return InsertResultShared::Success;
            }
            Some(mut old_entry) => {
                // the callee has added an element to the hashmap, but it
                // clashed with something. We'll have to keep track of it and
                // we should fix it
                //
                // By definition, we know that we get the 'old_entry' only if it
                // is in the same cache_id as us
                // Also, we don't have to check the LRU size since here the
                // number of elements remains the same

                just_inserted.user_on_insert(Some(&mut old_entry));
                // The clash was on something in our own cache.
                // In this case the head or tail might need to be changed

                self._scan.check_and_next((&old_entry).into());
                self._scan.apply_next();
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
                                self._tail = Some(just_inserted.into());
                            }
                            Some(mut old_entry_tail) => {
                                unsafe {
                                    // None    == old_entry.head
                                    // Some(_) == old_entry.tail
                                    // the head of the LRU
                                    old_entry_tail.as_mut().set_head_ptr(Some(
                                        just_inserted.into(),
                                    ));
                                }
                            }
                        }
                        self._head = Some(just_inserted.into());
                        return InsertResultShared::OldEntry(old_entry);
                    }
                    Some(mut old_entry_head) => {
                        match old_entry.get_tail_ptr() {
                            None => {
                                // Some(_) == old_entry.head
                                // None    == old_entry.tail
                                // we removed the old tail, with a hash clash
                                // the new tail is the prev of the old tail.
                                unsafe {
                                    old_entry_head.as_mut().set_tail_ptr(None);
                                    self._head.unwrap().as_mut().set_head_ptr(
                                        Some(just_inserted.into()),
                                    );
                                }
                                self._head = Some(just_inserted.into());
                                self._tail = Some(old_entry_head);
                                return InsertResultShared::OldTail(old_entry);
                            }
                            Some(mut old_entry_tail) => {
                                // Some(_) == old_entry.head
                                // Some(_) == old_entry.tail
                                // we removed something in the middle
                                unsafe {
                                    old_entry_tail
                                        .as_mut()
                                        .set_head_ptr(old_entry.get_head_ptr());
                                    old_entry
                                        .get_head_ptr()
                                        .unwrap()
                                        .as_mut()
                                        .set_tail_ptr(old_entry.get_tail_ptr());
                                    self._head.unwrap().as_mut().set_head_ptr(
                                        Some(just_inserted.into()),
                                    );
                                }
                                self._head = Some(just_inserted.into());
                                return InsertResultShared::OldEntry(old_entry);
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
        self._scan.stop();
    }
    pub fn remove_shared(&mut self, entry: &E) {
        self._scan.check_and_next(entry.into());
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
                Some(mut entry_tail) => {
                    // None == entry.head
                    // Some(_) == entry.tail
                    // we removed the head
                    unsafe {
                        entry_tail.as_mut().set_head_ptr(None);
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
                        entry
                            .get_head_ptr()
                            .unwrap()
                            .as_mut()
                            .set_tail_ptr(None);
                    }
                    self._tail = entry.get_head_ptr();
                }
                Some(mut entry_tail) => {
                    // Some(_) == entry.head
                    // Some(_) == entry.tail
                    // we removed an intermediate entry
                    unsafe {
                        entry_tail.as_mut().set_head_ptr(entry.get_head_ptr());
                        entry
                            .get_head_ptr()
                            .unwrap()
                            .as_mut()
                            .set_tail_ptr(entry.get_tail_ptr());
                    }
                }
            }
        }
    }
    /// make the key the head of the LRU.
    pub fn make_head(&mut self, entry: &mut E) {
        self._scan.check_and_next(entry.into());
        match entry.get_head_ptr() {
            None => {
                // already the head, nothing to do
            }
            Some(mut entry_head) => {
                unsafe {
                    entry_head.as_mut().set_tail_ptr(entry.get_tail_ptr());
                }
                match entry.get_tail_ptr() {
                    None => {
                        // we moved the tail to the head.
                        unsafe {
                            self._head
                                .unwrap()
                                .as_mut()
                                .set_head_ptr(Some(entry.into()));
                            entry.set_tail_ptr(self._head);
                        }
                        self._head = Some(entry.into());
                        self._tail = Some(entry_head);
                    }
                    Some(mut entry_tail) => {
                        // we promoted to head something in the middle
                        // of the linked list
                        unsafe {
                            entry_tail.as_mut().set_head_ptr(Some(entry_head));
                            entry_head.as_mut().set_tail_ptr(Some(entry_tail));
                            self._head
                                .unwrap()
                                .as_mut()
                                .set_head_ptr(Some(entry.into()));
                        }
                        self._head = Some(entry.into());
                    }
                }
            }
        }
    }
    pub fn get_cache_id(&self) -> Cid {
        self._cache_id
    }
    pub fn on_get(&mut self, entry: &mut E) {
        entry.user_on_get();
        self._scan.apply_next();
    }
    pub fn start_scan(&mut self) {
        match self._scan.is_running() {
            false => match self._head {
                Some(head) => self._scan.start_scan(head),
                None => {}
            },
            true => {}
        }
    }
    pub fn is_scan_running(&self) -> bool {
        self._scan.is_running()
    }
}
