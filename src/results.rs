#[derive(::thiserror::Error, Debug)]
pub enum Error {
    #[error("Key not found in lru")]
    KeyNotFound,
}

pub enum InsertResult<K, V> {
    OldEntry(K, V),
    OldTail(K, V),
    Success,
}

