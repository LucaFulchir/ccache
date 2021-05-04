# CCache, Composable Caches

Some caches are a combination of multiple others
This project tries to create a minimal framework to implement such caches, and
provides a common hashmap and traits to reuse and reimplement everything in any
way you want.

We also provide an (optional) way to store other metadata along with the values
in the cache and have callbacks triggered for every get/insert operation

## Current caches implemented

* LRU
* SLRU
* W-TiniyLFU (scan variant, named SW-TinyLFU

# ScanWindow Tiny LFU

The basic idea is to implement a w-tlfu cache as per
[this paper](https://arxiv.org/abs/1512.00727)

W-TLFU works by having counters which are all halved every X accesses.  
This seems problematic for larger caches, so instead we implemented a lazy scan:
instead of just a counter, each object keeps a generation (`Day/Night`) along
with the counter.  
Every time an object is accessed we scan it and the next one.  
If the generation is not the current one, the counter is halved.

Due to memory alignment SW-TLFU does not implement the "doorkeeper" bloom filter
and stores the counters directly in the hashmap used by the caches.

# Status/Help needed

* Shared LRU/SLRU/SW-TLFU done
* completely untested
* not benchmarked
* We can probably simplify the pointers in the `user::Entry` with smaller indexes
* some use of `unsafe` that I hope could be resolved but am not knowledgeable
  enough in rust
* More documentation needed
* common traits for all caches
* more advanced methods that just get/insert?
* common allocators
* wrappers to have templates with less parameters

# Structure:

### Hashmap

We had to reimplement an hashmap since the standard ones do not let you access
its entries by index, and are not stable (the objects can move around on
insert/remove)

The implementation is based on the same `RawTable` used by `hashbrown`

### `user::Entry`

You can reimplement you own entry type, assuming you provide the necessary
traits

You can add or better hide some fields, like `swtlfu::Full32` does to cram a
Generation and a counter in the cache id

Entries must know in which sub-cache they are, but if you are only using one
cache you can use `PhantomData`

### SharedXXX

This is the implementation of a shared cache. By Shared cache we mean one or
more probably more caches that reuse the same (shared) hashmap

