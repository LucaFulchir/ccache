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

// We only have two generations to keep track of
// There is no "new" and "old" generation, since
// every X queries the "old" will become the "new"
// so the naming should not give old/new ideas
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

static CounterMask: u32 = u32::MAX >> 1;
static GenerationMask: u32 = !CounterMask;

// Counter can go up to 2^31. the last bit is reserved to track the generation
//
pub struct Full {
    counter: u32,
}

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
