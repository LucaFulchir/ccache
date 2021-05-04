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

/// Standard `Hash` type, plus `Sized`, `Clone`, `Eq`, `Default`
pub trait Hash: Sized + Clone + ::std::hash::Hash + Eq + Default {}
/// The actual value in the hashmap: `Sized` and `Default`
pub trait Val: Sized + Default {}
/// The Cache-Id, which will tell to which cache an element belongs to
///
/// `Eq`, `Copy`, `Clone`, `Default`
pub trait Cid: Eq + Copy + Clone + Default {}

impl<T> Cid for ::std::marker::PhantomData<T> {}

/// The trait UserMeta defines operations that will be run on certain operations
/// of the LRU
pub trait Meta<V>: Default {
    /// create a new metadata struct with default values
    /// used if you don't want to specify one on insert(...)
    fn new() -> Self
    where
        Self: Sized;
    /// run every time the key is added or re-added
    /// as extra parameters you have:
    /// * old_meta: ref to the old metadata. used when you are re-adding the
    ///   same key, so that you can decide if you want to keep the old meta or
    ///   start anew
    /// * val: if somehow you need to modify the value every time we have an
    ///   access
    fn on_insert(
        &mut self,
        current_val: &mut V,
        old_entry: Option<(&Self, &mut V)>,
    );
    /// run every time the key is requested
    fn on_get(&mut self, val: &mut V);
}

/// The simplest of implementation for metadata:
/// No metadata, don't take up space and don't  do anything
#[derive(Default)]
pub struct ZeroMeta {}

impl<V> Meta<V> for ZeroMeta {
    fn new() -> Self {
        ZeroMeta {}
    }
    fn on_insert(
        &mut self,
        _current_val: &mut V,
        _old_entry: Option<(&Self, &mut V)>,
    ) {
    }
    fn on_get(&mut self, _val: &mut V) {}
}
// TODO: make 'head' and 'tail' typesafe.
// Does this require a full reimplementation of all pointer operations?

/// Trait to reimplement for the hashmap Entry

pub trait EntryT<K, V, Cid, Umeta>: Default
where
    K: Default,
    V: Val,
    Cid: crate::hashmap::user::Cid,
    Umeta: Meta<V>,
    Self: Sized,
{
    /// returns an entry with the given values
    fn new_entry(
        head: Option<::std::ptr::NonNull<Self>>,
        tail: Option<::std::ptr::NonNull<Self>>,
        key: K,
        val: V,
        cache_id: Cid,
        user_data: Umeta,
    ) -> Self;
    /// Get the pointer to a value higher in the cache
    fn get_head_ptr(&self) -> Option<::std::ptr::NonNull<Self>>;
    /// Set the pointer to a value higher in the cacheì
    fn set_head_ptr(&mut self, head: Option<::std::ptr::NonNull<Self>>);

    /// Get the pointer to a value lower in the cache
    fn get_tail_ptr(&self) -> Option<::std::ptr::NonNull<Self>>;
    /// Set the pointer to a value lower in the cacheì
    fn set_tail_ptr(&mut self, tail: Option<::std::ptr::NonNull<Self>>);

    /// get a reference to the key
    fn get_key(&self) -> &K;

    /// get a reference to the value
    fn get_val(&self) -> &V;
    /// get a mutable reference to the value
    fn get_val_mut(&mut self) -> &mut V;

    /// get the cache id
    fn get_cache_id(&self) -> Cid;
    /// get a mutable cache id
    fn get_cache_id_mut(&mut self) -> &mut Cid;
    /// return all the basic components of an entry
    fn deconstruct(self) -> (K, V, Umeta);

    /// get a reference to the metadata for the entry
    fn get_user(&self) -> &Umeta;
    /// get a mutable reference to the metadata for the entry
    fn get_user_mut(&mut self) -> &mut Umeta;

    /// get mutable references to both value and metadata
    fn get_val_user_mut(&mut self) -> (&mut V, &mut Umeta);

    /// Run the on-insert callback on this entry.
    ///
    /// Optionally get a ref to the old value if there was a clash
    fn user_on_insert(&mut self, old_entry: Option<&mut Self>);
    /// Run the on-get callback on the entry
    fn user_on_get(&mut self);

    /*
    unsafe fn from_val(val: &V) -> &Self;
    unsafe fn from_val_mut(val: &mut V) -> &mut Self;
    */
}

/// current implementation of our hashmap entries
///
/// Has two [`std::ptr::NonNull`] pointer  that the caches can use to reorder
/// the elements
pub struct Entry<K, V, Cid, Umeta>
where
    Umeta: Meta<V>,
    Cid: Copy,
{
    cache_id: Cid,
    // linked list towards head
    ll_head: Option<::std::ptr::NonNull<Self>>,
    // linked list towards tail
    ll_tail: Option<::std::ptr::NonNull<Self>>,
    key: K,
    val: V,
    user_data: Umeta,
}
impl<K, V, Cid, Umeta: Meta<V>> Default for Entry<K, V, Cid, Umeta>
where
    K: Hash,
    V: Val,
    Cid: crate::hashmap::user::Cid,
{
    fn default() -> Self {
        Entry {
            cache_id: Cid::default(),
            ll_head: None,
            ll_tail: None,
            key: K::default(),
            val: V::default(),
            user_data: Umeta::default(),
        }
    }
}

impl<K, V, Cid, Umeta: Meta<V>> EntryT<K, V, Cid, Umeta>
    for Entry<K, V, Cid, Umeta>
where
    K: Hash,
    V: Val,
    Cid: crate::hashmap::user::Cid,
{
    fn new_entry(
        head: Option<::std::ptr::NonNull<Self>>,
        tail: Option<::std::ptr::NonNull<Self>>,
        key: K,
        val: V,
        cache_id: Cid,
        user_data: Umeta,
    ) -> Self {
        Entry {
            cache_id: cache_id,
            ll_head: head,
            ll_tail: tail,
            key: key,
            val: val,
            user_data: user_data,
        }
    }
    fn get_head_ptr(&self) -> Option<::std::ptr::NonNull<Self>> {
        self.ll_head
    }
    fn set_head_ptr(&mut self, head: Option<::std::ptr::NonNull<Self>>) {
        self.ll_head = head;
    }
    fn get_tail_ptr(&self) -> Option<::std::ptr::NonNull<Self>> {
        self.ll_tail
    }
    fn set_tail_ptr(&mut self, tail: Option<::std::ptr::NonNull<Self>>) {
        self.ll_tail = tail;
    }
    fn get_key(&self) -> &K {
        &self.key
    }

    fn get_val(&self) -> &V {
        &self.val
    }
    fn get_val_mut(&mut self) -> &mut V {
        &mut self.val
    }
    fn get_cache_id(&self) -> Cid {
        self.cache_id
    }
    fn get_cache_id_mut(&mut self) -> &mut Cid {
        &mut self.cache_id
    }
    fn get_user(&self) -> &Umeta {
        &self.user_data
    }
    fn get_user_mut(&mut self) -> &mut Umeta {
        &mut self.user_data
    }
    fn get_val_user_mut(&mut self) -> (&mut V, &mut Umeta) {
        (&mut self.val, &mut self.user_data)
    }
    fn deconstruct(self) -> (K, V, Umeta) {
        (self.key, self.val, self.user_data)
    }
    fn user_on_insert(&mut self, old_entry: Option<&mut Self>) {
        match old_entry {
            None => self.user_data.on_insert(&mut self.val, None),
            Some(old_meta) => self.user_data.on_insert(
                &mut self.val,
                Some((&mut old_meta.user_data, &mut old_meta.val)),
            ),
        }
    }
    fn user_on_get(&mut self) {
        self.user_data.on_get(&mut self.val)
    }
}
/*
struct EntryIt<K, V, Cid, Umeta>
where
    Umeta: Meta<V>,
    Cid: Copy,
{
    e: Option<::std::ptr::NonNull<Entry<K, V, Cid, Umeta>>>,
}

impl<K, V, Cid, Umeta: Meta<V>> Iterator for EntryIt<K, V, Cid, Umeta>
where
    Umeta: Meta<V>,
    Cid: Copy,
{
    type Item = ::std::ptr::NonNull<EntryIt<K, V, Cid, Umeta>>;

    fn next(
        &mut self,
    ) -> Option<::std::ptr::NonNull<EntryIt<K, V, Cid, Umeta>>> {
        self.get_tail_ptr()
    }
}
*/
