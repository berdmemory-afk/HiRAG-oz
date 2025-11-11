//! LRU cache with TTL for decoded OCR results

use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use super::models::{DecodeResult, FidelityLevel};

/// Cache key combining region ID and fidelity
#[derive(Debug, Clone, Hash, Eq, PartialEq)]
struct CacheKey {
    region_id: String,
    fidelity: String,
}

/// Cache entry with TTL
#[derive(Debug, Clone)]
struct CacheEntry {
    result: DecodeResult,
    inserted_at: Instant,
}

/// LRU cache for decoded OCR results
pub struct DecodeCache {
    entries: Arc<Mutex<HashMap<CacheKey, CacheEntry>>>,
    ttl: Duration,
    max_size: usize,
}

impl DecodeCache {
    /// Create a new cache with TTL and max size
    pub fn new(ttl: Duration, max_size: usize) -> Self {
        Self {
            entries: Arc::new(Mutex::new(HashMap::new())),
            ttl,
            max_size,
        }
    }

    /// Get a cached result if available and not expired
    pub fn get(&self, region_id: &str, fidelity: &FidelityLevel) -> Option<DecodeResult> {
        let key = CacheKey {
            region_id: region_id.to_string(),
            fidelity: fidelity.as_str().to_string(),
        };

        let mut entries = self.entries.lock().unwrap();
        
        if let Some(entry) = entries.get(&key) {
            if entry.inserted_at.elapsed() < self.ttl {
                return Some(entry.result.clone());
            } else {
                // Expired, remove it
                entries.remove(&key);
            }
        }
        
        None
    }

    /// Store a result in the cache
    pub fn store(&self, region_id: &str, fidelity: &FidelityLevel, result: DecodeResult) {
        let key = CacheKey {
            region_id: region_id.to_string(),
            fidelity: fidelity.as_str().to_string(),
        };

        let entry = CacheEntry {
            result,
            inserted_at: Instant::now(),
        };

        let mut entries = self.entries.lock().unwrap();
        
        // Evict oldest if at capacity
        if entries.len() >= self.max_size && !entries.contains_key(&key) {
            self.evict_oldest(&mut entries);
        }
        
        entries.insert(key, entry);
    }

    /// Store multiple results
    pub fn store_batch(&self, results: &[DecodeResult], fidelity: &FidelityLevel) {
        for result in results {
            self.store(&result.region_id, fidelity, result.clone());
        }
    }

    /// Split region IDs into cache hits and misses
    pub fn split_hits(
        &self,
        region_ids: &[String],
        fidelity: &FidelityLevel,
    ) -> (Vec<DecodeResult>, Vec<String>) {
        let mut hits = Vec::new();
        let mut misses = Vec::new();

        for region_id in region_ids {
            if let Some(result) = self.get(region_id, fidelity) {
                hits.push(result);
            } else {
                misses.push(region_id.clone());
            }
        }

        (hits, misses)
    }

    /// Evict the oldest entry
    fn evict_oldest(&self, entries: &mut HashMap<CacheKey, CacheEntry>) {
        if let Some(oldest_key) = entries
            .iter()
            .min_by_key(|(_, entry)| entry.inserted_at)
            .map(|(key, _)| key.clone())
        {
            entries.remove(&oldest_key);
        }
    }

    /// Clear expired entries
    pub fn clear_expired(&self) {
        let mut entries = self.entries.lock().unwrap();
        entries.retain(|_, entry| entry.inserted_at.elapsed() < self.ttl);
    }

    /// Get cache statistics
    pub fn stats(&self) -> CacheStats {
        let entries = self.entries.lock().unwrap();
        let valid_count = entries
            .values()
            .filter(|entry| entry.inserted_at.elapsed() < self.ttl)
            .count();

        CacheStats {
            total_entries: entries.len(),
            valid_entries: valid_count,
            expired_entries: entries.len() - valid_count,
        }
    }
}

/// Cache statistics
#[derive(Debug, Clone)]
pub struct CacheStats {
    pub total_entries: usize,
    pub valid_entries: usize,
    pub expired_entries: usize,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cache_store_and_get() {
        let cache = DecodeCache::new(Duration::from_secs(60), 100);
        let fidelity = FidelityLevel::Medium;
        
        let result = DecodeResult {
            region_id: "region1".to_string(),
            text: "Hello World".to_string(),
            confidence: 0.95,
        };

        cache.store("region1", &fidelity, result.clone());
        
        let retrieved = cache.get("region1", &fidelity);
        assert!(retrieved.is_some());
        assert_eq!(retrieved.unwrap().text, "Hello World");
    }

    #[test]
    fn test_cache_expiration() {
        let cache = DecodeCache::new(Duration::from_millis(100), 100);
        let fidelity = FidelityLevel::Medium;
        
        let result = DecodeResult {
            region_id: "region1".to_string(),
            text: "Hello World".to_string(),
            confidence: 0.95,
        };

        cache.store("region1", &fidelity, result);
        
        // Should be available immediately
        assert!(cache.get("region1", &fidelity).is_some());
        
        // Wait for expiration
        std::thread::sleep(Duration::from_millis(150));
        
        // Should be expired
        assert!(cache.get("region1", &fidelity).is_none());
    }

    #[test]
    fn test_cache_split_hits() {
        let cache = DecodeCache::new(Duration::from_secs(60), 100);
        let fidelity = FidelityLevel::Medium;
        
        // Store some results
        cache.store("region1", &fidelity, DecodeResult {
            region_id: "region1".to_string(),
            text: "Text 1".to_string(),
            confidence: 0.95,
        });
        
        cache.store("region2", &fidelity, DecodeResult {
            region_id: "region2".to_string(),
            text: "Text 2".to_string(),
            confidence: 0.90,
        });

        // Query with mix of cached and uncached
        let region_ids = vec![
            "region1".to_string(),
            "region2".to_string(),
            "region3".to_string(),
        ];

        let (hits, misses) = cache.split_hits(&region_ids, &fidelity);
        
        assert_eq!(hits.len(), 2);
        assert_eq!(misses.len(), 1);
        assert_eq!(misses[0], "region3");
    }

    #[test]
    fn test_cache_eviction() {
        let cache = DecodeCache::new(Duration::from_secs(60), 2);
        let fidelity = FidelityLevel::Medium;
        
        // Fill cache to capacity
        cache.store("region1", &fidelity, DecodeResult {
            region_id: "region1".to_string(),
            text: "Text 1".to_string(),
            confidence: 0.95,
        });
        
        cache.store("region2", &fidelity, DecodeResult {
            region_id: "region2".to_string(),
            text: "Text 2".to_string(),
            confidence: 0.90,
        });

        // Add one more - should evict oldest
        cache.store("region3", &fidelity, DecodeResult {
            region_id: "region3".to_string(),
            text: "Text 3".to_string(),
            confidence: 0.85,
        });

        let stats = cache.stats();
        assert_eq!(stats.total_entries, 2);
    }
}