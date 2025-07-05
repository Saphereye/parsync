"""
Bandwidth monitoring and adaptive thread management
"""

import time
from typing import List, Tuple


class BandwidthMonitor:
    """Monitor bandwidth usage and provide recommendations for thread count"""
    
    def __init__(self, window_size: int = 10):
        self.window_size = window_size
        self.samples: List[Tuple[float, int, float]] = []  # (timestamp, bytes_transferred, bandwidth)
        self.start_time = time.time()
        
    def update(self, bytes_transferred: int) -> None:
        """Update bandwidth monitoring with new transfer data"""
        current_time = time.time()
        
        # Remove old samples outside the window
        cutoff_time = current_time - self.window_size
        self.samples = [s for s in self.samples if s[0] > cutoff_time]
        
        # Calculate bandwidth if we have previous samples
        if self.samples:
            time_diff = current_time - self.samples[-1][0]
            bytes_diff = bytes_transferred - self.samples[-1][1]
            if time_diff > 0:
                bandwidth = bytes_diff / time_diff
                self.samples.append((current_time, bytes_transferred, bandwidth))
        else:
            self.samples.append((current_time, bytes_transferred, 0))
    
    def get_bandwidth(self) -> float:
        """Get current bandwidth in bytes per second"""
        if len(self.samples) < 2:
            return 0
        
        # Use recent samples for more accurate bandwidth calculation
        recent_samples = self.samples[-5:]
        if len(recent_samples) < 2:
            return 0
            
        total_bytes = recent_samples[-1][1] - recent_samples[0][1]
        total_time = recent_samples[-1][0] - recent_samples[0][0]
        
        return total_bytes / total_time if total_time > 0 else 0
    
    def get_average_bandwidth(self) -> float:
        """Get average bandwidth over all samples"""
        if len(self.samples) < 2:
            return 0
        
        total_bytes = self.samples[-1][1] - self.samples[0][1]
        total_time = self.samples[-1][0] - self.samples[0][0]
        
        return total_bytes / total_time if total_time > 0 else 0
    
    def recommend_thread_count(self, current_threads: int, is_remote: bool = False) -> int:
        """Recommend optimal thread count based on bandwidth efficiency"""
        if not is_remote:
            return current_threads
            
        current_bandwidth = self.get_bandwidth()
        
        # If we don't have enough data, keep current threads
        if current_bandwidth <= 0 or len(self.samples) < 3:
            return current_threads
        
        # Calculate bandwidth per thread
        bandwidth_per_thread = current_bandwidth / current_threads
        
        # Thresholds for thread adjustment
        min_bandwidth_per_thread = 512 * 1024  # 512 KB/s minimum per thread
        optimal_bandwidth_per_thread = 2 * 1024 * 1024  # 2 MB/s optimal per thread
        
        if bandwidth_per_thread < min_bandwidth_per_thread and current_threads > 1:
            # Bandwidth per thread is too low, reduce threads
            return max(1, current_threads - 1)
        elif bandwidth_per_thread > optimal_bandwidth_per_thread and current_threads < 8:
            # We have good bandwidth, could potentially add more threads
            return min(8, current_threads + 1)
        
        return current_threads
    
    def get_stats(self) -> dict:
        """Get bandwidth statistics"""
        current_bw = self.get_bandwidth()
        avg_bw = self.get_average_bandwidth()
        
        return {
            'current_bandwidth_mbps': current_bw / (1024 * 1024),
            'average_bandwidth_mbps': avg_bw / (1024 * 1024),
            'sample_count': len(self.samples),
            'monitoring_duration': time.time() - self.start_time if self.samples else 0
        }
