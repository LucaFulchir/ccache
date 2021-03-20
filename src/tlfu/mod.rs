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

// FIXME: make the generation counter a 0/1, then keep a pointer to the
// last-reset counter. each access will check and halve just one more element
// this will mean that after `W` operations we have halved the whole counters
// and don't need to keep all generations
pub struct TLFU<K, V, U, HB>
where
    U: user::Meta<V>,
{
    _capacity: usize,
    _reset_counters: counter::Full,
    _doorkeeper: ::bitvec::vec::BitVec<Msb0, u64>,
    _counters: ::std::vec::Vec<counter::Full>,
    _slru: crate::slru::SLRU<K, V, U, HB>,
}
