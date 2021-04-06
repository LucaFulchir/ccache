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

/// The trait UserMeta defines operations that will be run on certain operations
/// of the LRU
pub trait Meta<V> {
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
pub trait EntryT<K, V, Cid, Umeta>
where
    Umeta: Meta<V>,
    Self: Sized,
{
    fn new_entry(
        head: Option<::std::ptr::NonNull<Self>>,
        tail: Option<::std::ptr::NonNull<Self>>,
        key: K,
        val: V,
        cache_id: Cid,
        user_data: Umeta,
    ) -> Self;
    fn get_head_ptr(&self) -> Option<::std::ptr::NonNull<Self>>;
    fn set_head_ptr(&mut self, head: Option<::std::ptr::NonNull<Self>>);

    fn get_tail_ptr(&self) -> Option<::std::ptr::NonNull<Self>>;
    fn set_tail_ptr(&mut self, tail: Option<::std::ptr::NonNull<Self>>);

    fn get_key(&self) -> &K;

    fn get_val(&self) -> &V;
    fn get_val_mut(&mut self) -> &mut V;

    fn get_cache_id(&self) -> Cid;
    fn get_cache_id_mut(&mut self) -> &mut Cid;
    fn deconstruct(self) -> (K, V, Umeta);

    fn get_user(&self) -> &Umeta;
    fn get_user_mut(&mut self) -> &mut Umeta;

    fn get_val_user_mut(&mut self) -> (&mut V, &mut Umeta);

    fn user_on_insert(&mut self, old_entry: Option<&mut Self>);
}

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
impl<K, V, Cid, Umeta: Meta<V>> EntryT<K, V, Cid, Umeta>
    for Entry<K, V, Cid, Umeta>
where
    Cid: Copy,
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
            ll_head: head,
            ll_tail: tail,
            key: key,
            val: val,
            cache_id: cache_id,
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
}
