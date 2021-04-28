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
use std::hash::Hasher;

/// Use this trait to wrap your hashmap implementation
/// We need this since the stdlib does not implement the methods
/// This will be all used in a single-thread context

// TODO: resizing
pub trait HashMap<Entry, Key, Val, Cid, Umeta, BuildHasher>
where
    Entry: crate::user::EntryT<Key, Val, Cid, Umeta>,
    Key: crate::user::Hash,
    Val: crate::user::Val,
    Cid: crate::user::Cid,
    Umeta: crate::user::Meta<Val>,
    BuildHasher: ::std::hash::BuildHasher,
{
    /// Construct an Hashmap with the given capacity
    fn with_capacity(capacity: usize) -> Self;
    /// Construct an Hashmap with the given capacity and hash builder
    fn with_capacity_and_hasher(
        capacity: usize,
        hash_builder: BuildHasher,
    ) -> Self;
    /// return the current hashmap max capacity
    fn capacity(&self) -> usize;
    /// Returns the current number of elements in the hashmap
    fn len(&self) -> usize;
    /// Get the index and the reference to an element if present
    fn get_full(&self, key: &Key) -> Option<(usize, &Entry)>;
    /// Get the index and a mutable reference to an element if present
    fn get_full_mut(&mut self, key: &Key) -> Option<(usize, &mut Entry)>;
    /// Return a reference to the object at the given index, if any
    fn get_index(&self, idx: usize) -> Option<&Entry>;
    /// Return a reference to the object at the given index, if any
    fn get_index_mut(&mut self, idx: usize) -> Option<&mut Entry>;
    /// Get a ref to an Entry and translate it to an index
    unsafe fn index_from_entry(&self, e: &Entry) -> usize;
    /// Remove and ojbect.
    /// Returns the removed object
    /// Must not reshuffle after removal
    fn remove(&mut self, item: &Entry) -> Entry;
    /// Remove and ojbect at idx.
    /// Returns the removed object
    /// If no object was present, return a default object
    /// Must not reshuffle after removal
    fn remove_idx(&mut self, idx: usize) -> Entry;
    /// Remove all objects in the hashmap
    fn clear(&mut self);
    /// Insert a new element in the hashmap.
    /// Returns a pair with a possible clash `Option<V>` plus the index and the
    /// reference to the object just inserted
    /// Must not reshuffle or reallocate
    fn insert(&mut self, entry: Entry) -> (Option<Entry>, usize, &Entry);
    /// Insert a new element in the hashmap.
    /// Returns a pair with a possible clash `Option<V>` plus the index and the
    /// mutable reference to the object just inserted
    /// Must not reshuffle or reallocate
    fn insert_mut(
        &mut self,
        entry: Entry,
    ) -> (Option<Entry>, usize, &mut Entry);
    /// returns a reference to the current hasher
    fn hasher(&self) -> &BuildHasher;
}

/// This simple hashmap has some limitations:
/// * It will not resize
/// * It always has the same maximum size
/// * Should not be used in multithread
/// But it should be efficient enugh, and stable
///
/// So if you add or remove elements, the other will not be reshuffled at any
/// time
///
/// It also supports O(1) access via index
///
/// Since it is built around `user::EntryT` you can even add metadata
/// that will receive callbacks on get/insert operations, and you can combine
/// multiple caches and distinguish them via the `Cid` type
///
/// In reaimplementing the needed types, remember that:
/// * `EntryT` must have a default type that will be used as "empty-space"
///   marker in the hash_map
/// * Cid need the default type which is used by EntryT to mark "empty-space"
// TODO: add allocator
pub struct SimpleHmap<
    Entry,
    Key,
    Val,
    Cid,
    Umeta,
    BuildHasher = std::collections::hash_map::RandomState,
> where
    Entry: crate::user::EntryT<Key, Val, Cid, Umeta>,
    Key: crate::user::Hash,
    Val: crate::user::Val,
    Cid: crate::user::Cid,
    Umeta: crate::user::Meta<Val>,
    BuildHasher: ::std::hash::BuildHasher + Default,
{
    usage: usize,
    table: ::hashbrown::raw::RawTable<Entry>,
    hash_builder: BuildHasher,
    _k: ::std::marker::PhantomData<Key>,
    _v: ::std::marker::PhantomData<Val>,
    _c: ::std::marker::PhantomData<Cid>,
    _u: ::std::marker::PhantomData<Umeta>,
}

impl<Entry, Key, Val, Cid, Umeta, BuildHasher>
    SimpleHmap<Entry, Key, Val, Cid, Umeta, BuildHasher>
where
    Entry: crate::user::EntryT<Key, Val, Cid, Umeta>,
    Key: crate::user::Hash,
    Val: crate::user::Val,
    Cid: crate::user::Cid,
    Umeta: crate::user::Meta<Val>,
    BuildHasher: ::std::hash::BuildHasher + Default,
{
    pub fn with_capacity(capacity: usize) -> Self {
        let mut res = SimpleHmap {
            usage: 0,
            table: ::hashbrown::raw::RawTable::<Entry>::with_capacity(capacity),
            hash_builder: BuildHasher::default(),
            _k: ::std::marker::PhantomData,
            _v: ::std::marker::PhantomData,
            _c: ::std::marker::PhantomData,
            _u: ::std::marker::PhantomData,
        };
        res.init_all_default(false);
        res
    }
    pub fn with_capacity_and_hasher(
        capacity: usize,
        hash_builder: BuildHasher,
    ) -> Self {
        let mut res = SimpleHmap {
            usage: 0,
            table: ::hashbrown::raw::RawTable::<Entry>::with_capacity(capacity),
            hash_builder: hash_builder,
            _k: ::std::marker::PhantomData,
            _v: ::std::marker::PhantomData,
            _c: ::std::marker::PhantomData,
            _u: ::std::marker::PhantomData,
        };
        res.init_all_default(false);
        res
    }
    fn init_all_default(&mut self, quick: bool) {
        match quick {
            false => {
                for idx in 0..(self.table.len()) {
                    unsafe {
                        let bucket = self.table.bucket(idx);
                        let default_el = Entry::default();
                        bucket.write(default_el);
                    }
                }
            }
            true => {
                for idx in 0..(self.table.len()) {
                    unsafe {
                        let bucket = self.table.bucket(idx);
                        let cid = bucket.as_mut().get_cache_id_mut();
                        *cid = Cid::default();
                    }
                }
            }
        }
        self.usage = 0;
    }
    pub fn capacity(&self) -> usize {
        self.table.capacity()
    }
    pub fn len(&self) -> usize {
        self.usage
    }
    fn hash(&self, key: &Key) -> u64 {
        let mut hasher = self.hash_builder.build_hasher();
        key.hash(&mut hasher);
        hasher.finish()
    }
    pub fn get_full(&self, key: &Key) -> Option<(usize, &Entry)> {
        let hash = self.hash(key);
        match self.table.find(hash, move |x| key.eq(x.get_key())) {
            None => None,
            Some(bucket) => {
                Some((unsafe { self.table.bucket_index(&bucket) }, unsafe {
                    bucket.as_ref()
                }))
            }
        }
    }
    pub fn get_full_mut(&mut self, key: &Key) -> Option<(usize, &mut Entry)> {
        let hash = self.hash(key);
        match self.table.find(hash, move |x| key.eq(x.get_key())) {
            None => None,
            Some(bucket) => {
                Some((unsafe { self.table.bucket_index(&bucket) }, unsafe {
                    bucket.as_mut()
                }))
            }
        }
    }
    pub fn get_index(&self, idx: usize) -> Option<&Entry> {
        if idx >= self.len() {
            return None;
        }
        let bucket = unsafe { self.table.bucket(idx) };
        if unsafe { bucket.as_ref() }.get_cache_id() == Cid::default() {
            return None;
        }
        Some(unsafe { bucket.as_ref() })
    }
    pub fn get_index_mut(&mut self, idx: usize) -> Option<&mut Entry> {
        if idx >= self.len() {
            return None;
        }
        let bucket = unsafe { self.table.bucket(idx) };
        if unsafe { bucket.as_ref() }.get_cache_id() == Cid::default() {
            return None;
        }
        Some(unsafe { bucket.as_mut() })
    }
    unsafe fn index_from_entry(&self, e: &Entry) -> usize {
        self.entry_to_idx(e)
    }
    fn entry_to_idx(&self, e: &Entry) -> usize {
        unsafe {
            // basically copied from the ::hashbrown::raw::Bucket implementation
            let ep = e as *const Entry;
            let end = self.table.data_end();
            end.as_ptr().offset_from(ep) as usize
        }
    }
    /// just mark the entry as not part of a `Cid` and return a copy
    /// don't bother actually deleting the data
    pub fn remove(&mut self, item: &Entry) -> Entry {
        let idx = self.entry_to_idx(item);
        self.remove_idx_unsafe(idx)
    }
    /// just mark the entry as not part of a `Cid` and return a copy
    /// don't bother actually deleting the data
    /// If not present, return a default `Entry`
    pub fn remove_idx(&mut self, idx: usize) -> Entry {
        if idx >= self.capacity() {
            return Entry::default();
        }
        self.remove_idx_unsafe(idx)
    }
    fn remove_idx_unsafe(&mut self, idx: usize) -> Entry {
        unsafe {
            let bucket = self.table.bucket(idx);
            if bucket.as_ref().get_cache_id() == Cid::default() {
                return Entry::default();
            }
            self.usage -= 1;
            let res: Entry = bucket.read();
            let old_e_cid = bucket.as_mut().get_cache_id_mut();
            *old_e_cid = Cid::default();
            res
        }
    }
    pub fn clear(&mut self) {
        self.init_all_default(true)
    }
    /// returns: any eventual clash in `Option<Entry>` plus the index and a ref
    /// to the actual entry.
    /// If the hashmap is full and we can not insert the element, returns Err
    pub fn insert(&mut self, entry: Entry) -> (Option<Entry>, usize, &Entry) {
        let (clash, idx, entry) = self.insert_mut(entry);
        (clash, idx, entry)
    }
    pub fn insert_mut(
        &mut self,
        entry: Entry,
    ) -> (Option<Entry>, usize, &mut Entry) {
        self.usage += 1;
        let hash = self.hash(entry.get_key());
        let non_mut_self = &*self;
        let opt_clash_strong = self.table.find(hash, move |x| {
            if (&x).get_cache_id() == Cid::default() {
                return true;
            }
            let k = x.get_key();
            let h2 = non_mut_self.hash(k);
            hash.eq(&h2)
        });
        let opt_clash_real =
            if opt_clash_strong.is_none() && (self.usage == self.capacity()) {
                // hashmap is full, but we did not find a clash.
                // since we always guarantee an insert, let's weaken the hash
                // and force a clash
                let weak_hash = hash % (self.capacity() as u64);
                self.table.find(weak_hash, move |x| {
                    if (&x).get_cache_id() == Cid::default() {
                        return true;
                    }
                    let k = x.get_key();
                    let h2 =
                        non_mut_self.hash(k) % (non_mut_self.capacity() as u64);
                    weak_hash.eq(&h2)
                })
            } else {
                opt_clash_strong
            };
        let (opt_clash_copy, bucket) = match opt_clash_real {
            None => {
                let bucket = self.table.insert_no_grow(hash, entry);
                let bucket_idx = unsafe { self.table.bucket_index(&bucket) };
                return (None, bucket_idx, unsafe { bucket.as_mut() });
            }
            Some(bucket) => {
                let old_element = if unsafe { bucket.as_ref() }.get_cache_id()
                    == Cid::default()
                {
                    None
                } else {
                    Some(unsafe { bucket.read() })
                };
                (old_element, bucket)
            }
        };
        unsafe {
            bucket.write(entry);
        }
        let bucket_idx = unsafe { self.table.bucket_index(&bucket) };
        (opt_clash_copy, bucket_idx, unsafe { bucket.as_mut() })
    }
    pub fn hasher(&self) -> &BuildHasher {
        &self.hash_builder
    }
}
impl<Entry, Key, Val, Cid, Umeta, BuildHasher>
    HashMap<Entry, Key, Val, Cid, Umeta, BuildHasher>
    for SimpleHmap<Entry, Key, Val, Cid, Umeta, BuildHasher>
where
    Entry: crate::user::EntryT<Key, Val, Cid, Umeta>,
    Key: crate::user::Hash,
    Val: crate::user::Val,
    Cid: crate::user::Cid,
    Umeta: crate::user::Meta<Val>,
    BuildHasher: ::std::hash::BuildHasher + Default,
{
    fn with_capacity(capacity: usize) -> Self {
        SimpleHmap::with_capacity(capacity)
    }
    fn with_capacity_and_hasher(
        capacity: usize,
        hash_builder: BuildHasher,
    ) -> Self {
        SimpleHmap::with_capacity_and_hasher(capacity, hash_builder)
    }
    fn capacity(&self) -> usize {
        SimpleHmap::capacity(self)
    }
    fn len(&self) -> usize {
        SimpleHmap::len(self)
    }
    fn get_full(&self, key: &Key) -> Option<(usize, &Entry)> {
        SimpleHmap::get_full(self, key)
    }
    fn get_full_mut(&mut self, key: &Key) -> Option<(usize, &mut Entry)> {
        SimpleHmap::get_full_mut(self, key)
    }
    fn get_index(&self, idx: usize) -> Option<&Entry> {
        SimpleHmap::get_index(self, idx)
    }
    fn get_index_mut(&mut self, idx: usize) -> Option<&mut Entry> {
        SimpleHmap::get_index_mut(self, idx)
    }
    unsafe fn index_from_entry(&self, e: &Entry) -> usize {
        SimpleHmap::index_from_entry(self, e)
    }
    fn remove(&mut self, item: &Entry) -> Entry {
        SimpleHmap::remove(self, item)
    }
    fn remove_idx(&mut self, idx: usize) -> Entry {
        SimpleHmap::remove_idx(self, idx)
    }
    fn clear(&mut self) {
        SimpleHmap::clear(self)
    }
    fn insert(&mut self, entry: Entry) -> (Option<Entry>, usize, &Entry) {
        SimpleHmap::insert(self, entry)
    }
    fn insert_mut(
        &mut self,
        entry: Entry,
    ) -> (Option<Entry>, usize, &mut Entry) {
        SimpleHmap::insert_mut(self, entry)
    }
    fn hasher(&self) -> &BuildHasher {
        SimpleHmap::hasher(self)
    }
}
