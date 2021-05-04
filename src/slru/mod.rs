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

//! Basic SLRU cache
//!
//! Extra features: callbacks on get/insert, lazy scan callback
//!
//! The lazy scan is a scan that only runs on one more element after each
//! get/insert  
//! This means that X elements will be fully scanned only after X get/insert

use crate::hashmap;
use crate::hashmap::user;
use crate::hashmap::user::EntryT;
use crate::results::{InsertResult, InsertResultShared};

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
impl user::Cid for SLRUCid {}

type SLRUEntry<K, V, Umeta> = user::Entry<K, V, SLRUCid, Umeta>;
type HmapT<K, V, Umeta, HB> =
    hashmap::SimpleHmap<SLRUEntry<K, V, Umeta>, K, V, SLRUCid, Umeta, HB>;

/// [SLRU](https://en.wikipedia.org/wiki/Cache_replacement_policies#Segmented_LRU_(SLRU))
/// implementation
///
/// SLRU is a Segmented LRU it consists of two LRU:
///  * probation LRU: for items that have been just added
///  * protected LRU: items that were in the probation LRU and received a HIT
///
/// W-TinyLFU specifies an 20-80 split, with 80% for the probation LRU
/// You can define your own slit here
pub struct SLRU<'a, K, V, Umeta, HB>
where
    K: user::Hash,
    V: user::Val,
    Umeta: user::Meta<V>,
    HB: ::std::hash::BuildHasher + Default,
{
    _hmap: HmapT<K, V, Umeta, HB>,
    _slru: SLRUShared<
        'a,
        HmapT<K, V, Umeta, HB>,
        SLRUEntry<K, V, Umeta>,
        K,
        V,
        SLRUCid,
        Umeta,
        HB,
    >,
}

impl<
        'a,
        K: user::Hash,
        V: user::Val,
        Umeta: user::Meta<V>,
        HB: ::std::hash::BuildHasher + Default,
    > SLRU<'a, K, V, Umeta, HB>
{
    /// new SLRU, with custom number of entries for the probatory and protected
    /// splits
    pub fn new(
        probation_entries: usize,
        protected_entries: usize,
        extra_hashmap_capacity: usize,
        hash_builder: HB,
    ) -> Self {
        SLRU {
            _hmap: HmapT::<K, V, Umeta, HB>::with_capacity_and_hasher(
                1 + probation_entries
                    + protected_entries
                    + extra_hashmap_capacity,
                hash_builder,
            ),
            _slru: SLRUShared::<
                'a,
                HmapT<K, V, Umeta, HB>,
                SLRUEntry<K, V, Umeta>,
                K,
                V,
                SLRUCid,
                Umeta,
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
                None,
            ),
        }
    }
    /// insert a new element. Can return a clash
    pub fn insert(&mut self, key: K, val: V) -> InsertResult<(K, V, Umeta)> {
        self.insert_with_meta(key, val, Umeta::new())
    }
    /// insert a new element, but with metadata
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
        let (mut maybe_old_entry, new_entry_idx, _new_entry) =
            self._hmap.insert(e);
        let maybe_ref_old = maybe_old_entry.as_mut();
        match self._slru.insert_shared(
            &mut self._hmap,
            maybe_ref_old,
            new_entry_idx,
        ) {
            InsertResultShared::OldEntry { evicted } => {
                let c = match maybe_old_entry {
                    None => None,
                    Some(x) => Some(x.deconstruct()),
                };
                let e = match evicted {
                    None => None,
                    Some(x) => Some(x.deconstruct()),
                };
                InsertResult::OldEntry {
                    clash: c,
                    evicted: e,
                }
            }
            InsertResultShared::OldTailPtr { evicted } => {
                let c = match maybe_old_entry {
                    None => None,
                    Some(x) => Some(x.deconstruct()),
                };
                let removed = self._hmap.remove(unsafe { &*evicted.as_ptr() });
                InsertResult::OldTail {
                    clash: c,
                    evicted: removed.deconstruct(),
                }
            }
            InsertResultShared::Success => InsertResult::Success,
        }
    }

    /// remove an element
    pub fn remove(&mut self, key: &K) -> Option<(V, Umeta)> {
        let (idx, entry) = match self._hmap.get_full(key) {
            None => return None,
            Some((idx, entry)) => (idx, entry),
        };
        self._slru.remove_shared(entry);
        let (_, val, meta) = self._hmap.remove_idx(idx).deconstruct();
        Some((val, meta))
    }
    /// clear out all the SLRU
    pub fn clear(&mut self) {
        self._hmap.clear();
        self._slru.clear_shared()
    }
    /// Get references to the element's data
    pub fn get(&mut self, key: &K) -> Option<(&V, &Umeta)> {
        match self._hmap.get_full_mut(key) {
            None => None,
            Some((_, entry)) => {
                self._slru.on_get(entry);
                Some((entry.get_val(), entry.get_user()))
            }
        }
    }
    /// get a mutable reference to the element's data
    pub fn get_mut(&mut self, key: &K) -> Option<(&mut V, &mut Umeta)> {
        match self._hmap.get_full_mut(key) {
            None => None,
            Some((_, entry)) => {
                self._slru.on_get(entry);
                Some(entry.get_val_user_mut())
            }
        }
    }
}
#[derive(PartialEq, Eq)]
enum ScanStatus {
    Stopped,
    RunningProbation,
    RunningProtected,
}

/// Actual implementation of the SLRU on a shared hashmap
///
/// Note that Insert/Remove do not actually insert anything in the hashmap.
/// That must be done by the caller. We only fix all the pointers and SLRU
/// status

pub struct SLRUShared<'a, Hmap, E, K, V, CidT, Umeta, HB>
where
    Hmap: hashmap::HashMap<E, K, V, CidT, Umeta, HB>,
    E: user::EntryT<K, V, CidT, Umeta>,
    K: user::Hash,
    V: user::Val,
    CidT: user::Cid,
    Umeta: user::Meta<V>,
    HB: ::std::hash::BuildHasher + Default,
{
    _probation: crate::lru::LRUShared<'a, Hmap, E, K, V, CidT, Umeta, HB>,
    _protected: crate::lru::LRUShared<'a, Hmap, E, K, V, CidT, Umeta, HB>,
    _scanstatus: ScanStatus,
}

impl<
        'a,
        Hmap: hashmap::HashMap<E, K, V, CidT, Umeta, HB>,
        E: user::EntryT<K, V, CidT, Umeta>,
        K: user::Hash,
        V: user::Val,
        CidT: user::Cid,
        Umeta: user::Meta<V>,
        HB: ::std::hash::BuildHasher + Default,
    > SLRUShared<'a, Hmap, E, K, V, CidT, Umeta, HB>
{
    /// new SLRU with custom number of probatory/protected entries
    /// plus an optional callback for the lazy scan
    pub fn new(
        probation: (usize, CidT),
        protected: (usize, CidT),
        access_scan: Option<&'a dyn Fn(::std::ptr::NonNull<E>) -> ()>,
    ) -> Self {
        SLRUShared {
            _probation: crate::lru::LRUShared::<
                'a,
                Hmap,
                E,
                K,
                V,
                CidT,
                Umeta,
                HB,
            >::new(
                probation.0, probation.1, access_scan.clone()
            ),
            _protected: crate::lru::LRUShared::<
                'a,
                Hmap,
                E,
                K,
                V,
                CidT,
                Umeta,
                HB,
            >::new(
                protected.0, protected.1, access_scan
            ),
            _scanstatus: ScanStatus::Stopped,
        }
    }
    /// change the scan callback
    pub fn set_scanf(
        &mut self,
        access_scan: Option<&'a dyn Fn(::std::ptr::NonNull<E>) -> ()>,
    ) {
        self._probation.set_scanf(access_scan);
        self._protected.set_scanf(access_scan)
    }
    /// an itam has been inserted by the caller, fix the SLRU
    ///
    /// `maybe_old_entry` must be `!= None` only if the element is in the SLRU
    pub fn insert_shared(
        &mut self,
        hmap: &mut Hmap,
        maybe_old_entry: Option<&mut E>,
        new_entry_idx: usize,
    ) -> InsertResultShared<E> {
        let just_inserted = hmap.get_index_mut(new_entry_idx).unwrap();
        if just_inserted.get_cache_id() == self._probation.get_cache_id() {
            // inserted twice. promote to protected
            // Note that since we already found the key, we can not have had any
            // clash (maybe_old_entry == None)
            self._probation.remove_shared(just_inserted);
            let res = match self._protected.insert_shared(
                hmap,
                None,
                new_entry_idx,
            ) {
                InsertResultShared::OldTailPtr { evicted } => {
                    // clash is always None
                    // when an insert causes a tail eviction in the protected
                    // segment, that has to be re-inserted in the probatory
                    self._probation.insert_shared(hmap, None, unsafe {
                        hmap.index_from_entry(&*evicted.as_ptr())
                    })
                }
                r @ _ => r,
            };
            self.update_scan_status();
            res
        } else if just_inserted.get_cache_id() == self._protected.get_cache_id()
        {
            // inserted more than once, in protected
            // Note that since we already found the key, we can not have had any
            // clash (maybe_old_entry == None) and there will be no cache
            // eviction
            let res = self._protected.insert_shared(hmap, None, new_entry_idx);
            self.update_scan_status();
            res
        } else {
            // new insert, not in any cache.
            // We might have had a clash, but we will insert into probation
            match maybe_old_entry {
                None => {
                    self._probation.insert_shared(hmap, None, new_entry_idx)
                }
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
                        let res = self._probation.insert_shared(
                            hmap,
                            Some(old_entry),
                            new_entry_idx,
                        );
                        self.update_scan_status();
                        res
                    } else {
                        self._protected.remove_shared(&old_entry);
                        let res = self._probation.insert_shared(
                            hmap,
                            None,
                            new_entry_idx,
                        );
                        self.update_scan_status();
                        res
                    }
                }
            }
        }
    }
    /// Reset the SLRU state
    pub fn clear_shared(&mut self) {
        self._probation.clear_shared();
        self._protected.clear_shared();
        self._scanstatus = ScanStatus::Stopped;
    }
    /// do not actually remove the element, just fix the SLRU so that it is not
    /// considered anymore and actual removal is safe
    pub fn remove_shared(&mut self, entry: &E) {
        let res = if entry.get_cache_id() == self._probation.get_cache_id() {
            self._probation.remove_shared(entry)
        } else {
            self._protected.remove_shared(entry)
        };
        self.update_scan_status();
        res
    }
    /// return the cache ids for `(probatory, protected)`
    pub fn get_cache_ids(&self) -> (CidT, CidT) {
        (
            self._probation.get_cache_id(),
            self._protected.get_cache_id(),
        )
    }
    /// Should be called only by the parent, run the on-get callback on the
    /// correct LRU
    pub fn on_get(&mut self, entry: &mut E) {
        if entry.get_cache_id() == self._probation.get_cache_id() {
            self._probation.on_get(entry);
        } else {
            self._protected.on_get(entry);
        }
        self.update_scan_status();
    }
    /// start the scan callbacks on the SLRU
    /// scan will execute only once on the whole SLRU
    pub fn start_scan(&mut self) {
        self._probation.start_scan();
        match self._probation.is_scan_running() {
            true => {
                self._scanstatus = ScanStatus::RunningProbation;
            }
            false => {
                self._protected.start_scan();
                match self._protected.is_scan_running() {
                    true => {
                        self._scanstatus = ScanStatus::RunningProtected;
                    }
                    false => {
                        self._scanstatus = ScanStatus::Stopped;
                    }
                }
            }
        }
    }
    /// return scan status
    pub fn is_scan_running(&self) -> bool {
        self._scanstatus != ScanStatus::Stopped
    }
    fn update_scan_status(&mut self) {
        match self._scanstatus {
            ScanStatus::Stopped => {}
            ScanStatus::RunningProbation => {
                match self._probation.is_scan_running() {
                    true => {}
                    false => {
                        self._protected.start_scan();
                        self._scanstatus = ScanStatus::RunningProtected;
                    }
                }
            }
            ScanStatus::RunningProtected => {
                match self._protected.is_scan_running() {
                    true => {}
                    false => {
                        self._scanstatus = ScanStatus::Stopped;
                    }
                }
            }
        }
    }
    /// return max SLRU size
    pub fn capacity(&self) -> usize {
        self._probation.capacity() + self._protected.capacity()
    }
    /// return used SLRU size
    pub fn len(&self) -> usize {
        self._probation.len() + self._protected.len()
    }
}
