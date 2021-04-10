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

mod counter;

use crate::results::InsertResult;
use crate::user;
use bitvec::prelude::*;

/// Tiny LFU cache works by having a first bloom filter, called "doorkeper".
/// This tracks all elements in the actual caches
/// After this first filter is passed we have a more detailed set of counters
/// The counters do not cover the whole cache, since we don't want to waste
/// space tracking lots and lots of single-use elements.
///
/// The counter is not just a normal frequency counter:
/// each time an element is added, the key is used to generate multiple
/// deterministic hashes. You check all the counters in those positions,
/// and on insert you increase by one all elemnts except for the maximum
///
/// This will give you a list of the most used or unused elements, that TLFU
/// will use to know which element to evict.
///
/// Behind TLFU is only one SLRU cache, with a 20/80 split: 20% on probation,
/// 80% on the protected split
///
/// Every W inserts TLFU says to scan the whole counter vector and halve all
/// elements, then clear out the doorkeper. Since that is a much longer
/// operation than normal, we will opt instead for a lazy approach:
/// * we keep increasing the main reset counter as normal
/// * past `W`, we set the main counter to 0 and increase the generation counter
/// * every time we access the counters, we check their generation. if it is
///   higher than the current, we halve as many times as necessary

/// TLFU will store the frequency in the cache id.
/// We do this since:
///  * we use a share hashmap so that we don't have to move elements from one
///    hashmap to the other
///  * a cache id is needed to to the previous point, otherwise we would not
///    know to chich cache an element belongs to
///  This means that we are already wasting bytes in memory.
///  We will put those bytes to use by storing the frequency of each element
///  together with the Cid
/// This way we don't even need the bloom filter
pub trait Freq {
    fn add(&mut self);
    fn halve(&mut self);
    fn clear(&mut self);
}

// FIXME: make the generation counter a 0/1, then keep a pointer to the
// last-reset counter. each access will check and halve just one more element
// this will mean that after `W` operations we have halved the whole counters
// and don't need to keep all generations
pub struct TLFUShared<E, K, V, Cid, Umeta, HB>
where
    E: user::EntryT<K, V, Cid, Umeta>,
    V: Sized,
    Cid: Eq + Copy + Freq,
    Umeta: user::Meta<V>,
    HB: ::std::hash::BuildHasher,
{
    _reset_counters: counter::Full,
    _doorkeeper: ::bitvec::vec::BitVec<Msb0, u64>,
    _counters: ::std::vec::Vec<counter::Full>,
    _slru: crate::slru::SLRUShared<E, K, V, Cid, Umeta, HB>,
}

impl<
        E: user::EntryT<K, V, Cid, Umeta>,
        K: ::std::hash::Hash + Clone + Eq,
        V,
        Cid: Eq + Copy + Freq,
        Umeta: user::Meta<V>,
        HB: ::std::hash::BuildHasher,
    > TLFUShared<E, K, V, Cid, Umeta, HB>
{
    pub fn new(
        entries: usize,
        cids: [Cid; 2],
    ) -> TLFUShared<E, K, V, Cid, Umeta, HB> {
        let (probation_entries, protected_entries) =
            match ((entries as f64) * 0.2) as usize {
                0 => {
                    if entries == 0 {
                        (0, 0)
                    } else {
                        (1, entries - 1)
                    }
                }
                x @ _ => (x, entries - x),
            };

        TLFUShared {
            _reset_counters: counter::Full::new(),
            _doorkeeper: ::bitvec::vec::BitVec::<Msb0, u64>::with_capacity(
                entries,
            ),
            _counters: ::std::vec::Vec::<counter::Full>::with_capacity(entries),
            _slru: crate::slru::SLRUShared::<E, K, V, Cid, Umeta, HB>::new(
                (probation_entries, cids[0]),
                (protected_entries, cids[1]),
            ),
        }
    }
    /*
    pub fn insert_shared(
        &mut self,
        hmap: &mut ::std::collections::HashMap<K, E, HB>,
        maybe_old_entry: Option<E>,
        key: &K,
    ) -> InsertResultShared<E, K> {
        let just_inserted = hmap.get_mut(&key).unwrap();
        *just_inserted.get_cache_id_mut() = self._cache_id;

        match maybe_old_entry {
            None => {
                just_inserted.user_on_insert(None);
            }
            Some(old_entry) => {}
        }
    }
    */
}
