use std::collections::{HashMap, VecDeque};

use diachron_core::SearchResult;

#[derive(Clone, Hash, PartialEq, Eq)]
pub struct CacheKey {
    pub query: String,
    pub limit: usize,
    pub source_filter: Option<u8>,
    pub since: Option<String>,
    pub project: Option<String>,
    pub db_version: String,
}

#[derive(Clone)]
pub struct CacheEntry {
    pub results: Vec<SearchResult>,
    pub embedding_used: bool,
}

pub struct SearchCache {
    capacity: usize,
    map: HashMap<CacheKey, CacheEntry>,
    order: VecDeque<CacheKey>,
}

impl SearchCache {
    pub fn new(capacity: usize) -> Self {
        Self {
            capacity: capacity.max(1),
            map: HashMap::new(),
            order: VecDeque::new(),
        }
    }

    pub fn get(&mut self, key: &CacheKey) -> Option<CacheEntry> {
        if let Some(entry) = self.map.get(key).cloned() {
            self.touch(key);
            return Some(entry);
        }
        None
    }

    pub fn insert(&mut self, key: CacheKey, entry: CacheEntry) {
        if self.map.contains_key(&key) {
            self.touch(&key);
            self.map.insert(key, entry);
            return;
        }

        self.order.push_back(key.clone());
        self.map.insert(key, entry);

        while self.map.len() > self.capacity {
            if let Some(old_key) = self.order.pop_front() {
                self.map.remove(&old_key);
            } else {
                break;
            }
        }
    }

    fn touch(&mut self, key: &CacheKey) {
        if let Some(pos) = self.order.iter().position(|k| k == key) {
            self.order.remove(pos);
        }
        self.order.push_back(key.clone());
    }
}
