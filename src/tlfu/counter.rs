use bitfield::bitfield;

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

// TODO: implement small counter optimization
// the idea will be to have a bitvector under us, and implement From<...>
// methods to load/save on the right bits

// We only have two generations to keep track of.
// There is no "new" and "old" generation, since
// every X queries the "old" will become the "new"
// The naming should not give old/new ideas
#[derive(PartialEq, Eq, Copy, Clone)]
enum Generation {
    Day,
    Night,
}
impl ::std::ops::Not for Generation {
    type Output = Self;
    fn not(self) -> Self::Output {
        match self {
            Generation::Day => Generation::Night,
            Generation::Night => Generation::Day,
        }
    }
}
impl From<bool> for Generation {
    fn from(is_night: bool) -> Self {
        match is_night {
            false => Generation::Day,
            true => Generation::Night,
        }
    }
}
impl From<Generation> for bool {
    fn from(g: Generation) -> Self {
        match g {
            Generation::Day => false,
            Generation::Night => true,
        }
    }
}

pub trait CidCounter<Cid>
where
    Cid: Eq + Copy,
{
    fn get_cid(&self) -> Cid;
    fn set_cid(&mut self, cid: Cid);

    fn get_generation(&self) -> Generation;
    fn flip_generation(&mut self);

    fn get_counter(&self) -> u32;
    fn add(&mut self);
    fn halve(&mut self);
}

#[derive(Debug, PartialEq, Eq, Copy, Clone)]
pub enum TLFUCid {
    None = 0,
    SLRUProbation = 1,
    SLRUProtected = 2,
}
impl Default for TLFUCid {
    fn default() -> Self {
        TLFUCid::None
    }
}
impl From<u8> for TLFUCid {
    fn from(raw: u8) -> Self {
        match raw {
            0 => TLFUCid::None,
            1 => TLFUCid::SLRUProbation,
            2 => TLFUCid::SLRUProtected,
            _ => {
                ::std::panic!("No such binary repr of TLFUCid")
            }
        }
    }
}

::bitfield::bitfield! {
    pub struct Full32(u32);
    impl Debug;
    #[inline]
    pub u8, into TLFUCid, get_cid, set_cid: 2, 0;
    #[inline]
    pub into Generation, get_generation, set_generation: 0;
    #[inline]
    pub u32, get_counter, set_counter: 29, 0;
}

impl CidCounter<TLFUCid> for Full32 {
    fn get_cid(&self) -> TLFUCid {
        self.get_cid()
    }
    fn set_cid(&mut self, cid: TLFUCid) {
        self.set_cid(cid as u8)
    }

    fn get_generation(&self) -> Generation {
        self.get_generation().into()
    }
    fn flip_generation(&mut self) {
        match self.get_generation().into() {
            Generation::Day => self.set_generation(Generation::Night.into()),
            Generation::Night => self.set_generation(Generation::Day.into()),
        }
    }

    fn get_counter(&self) -> u32 {
        self.get_counter()
    }
    fn add(&mut self) {
        let tmp = self.get_counter();
        self.set_counter(tmp + 1);
    }
    fn halve(&mut self) {
        let tmp = self.get_counter();
        self.set_counter(tmp / 2);
    }
}
// u32 with following bitfields, from the most significant:
//  * 2 bit: Cid
//  * 1 bit: Generation
//  * 29 bits: counter
static CidMask: u32 = !(u32::MAX >> 2);
static CounterMask: u32 = u32::MAX >> 3;
static GenerationMask: u32 = !(CounterMask | CidMask);

pub struct Full {
    counter: u32,
    //_cid: ::std::marker::PhantomData<Cid>;
}

impl Default for Full {
    #[inline]
    fn default() -> Self {
        Full { counter: 0 }
    }
}

//impl<Cid> CidCounter<Full> for Full<Cid> {
impl Full {
    #[inline]
    fn generation(&self) -> Generation {
        ((self.counter & GenerationMask) != 0).into()
    }
    #[inline]
    pub fn next(&mut self, max: &Full) {
        let currentGen = self.generation();
        match currentGen == max.generation() {
            true => self.counter += 1,
            false => {
                self.counter = (self.counter & CounterMask) / 2;
                match currentGen {
                    Generation::Day => {
                        // siwtch to day, MSB == 1
                        self.counter ^= CounterMask;
                    }
                    Generation::Night => {
                        // switch to day == MSB to 0
                        self.counter &= GenerationMask;
                    }
                }
            }
        }
    }
}
