"""
Job management and load balancing for smart rsync
"""

from dataclasses import dataclass
from typing import List


@dataclass
class FileInfo:
    """Information about a file to transfer"""
    path: str
    size: int
    relative_path: str


@dataclass
class TransferJob:
    """A job containing files to transfer"""
    files: List[FileInfo]
    total_size: int
    job_id: int


def create_balanced_jobs(files: List[FileInfo], max_threads: int, chunk_size_bytes: int) -> List[TransferJob]:
    """Create balanced transfer jobs using intelligent load balancing"""
    if not files:
        return []
    
    # Sort files by size (largest first) for better load balancing
    files.sort(key=lambda x: x.size, reverse=True)
    
    # Create initial jobs
    jobs = [TransferJob(files=[], total_size=0, job_id=i) for i in range(max_threads)]
    
    # Distribute files using a greedy approach with size balancing
    for file_info in files:
        # Find the job with the smallest current size
        min_job = min(jobs, key=lambda x: x.total_size)
        
        # Check if adding this file would exceed chunk size significantly
        if (min_job.total_size + file_info.size > chunk_size_bytes and 
            min_job.files and 
            file_info.size < chunk_size_bytes / 2):
            
            # Try to find a job with more room
            available_jobs = [j for j in jobs if j.total_size + file_info.size <= chunk_size_bytes]
            if available_jobs:
                target_job = min(available_jobs, key=lambda x: x.total_size)
                target_job.files.append(file_info)
                target_job.total_size += file_info.size
                continue
        
        # Add to the job with minimum size
        min_job.files.append(file_info)
        min_job.total_size += file_info.size
        
        # If this job is now very large, try to balance by moving smaller files
        if min_job.total_size > chunk_size_bytes * 1.5 and len(min_job.files) > 1:
            _rebalance_job(min_job, jobs, chunk_size_bytes)
    
    # Filter out empty jobs and renumber
    active_jobs = [job for job in jobs if job.files]
    for i, job in enumerate(active_jobs):
        job.job_id = i
    
    return active_jobs


def _rebalance_job(oversized_job: TransferJob, all_jobs: List[TransferJob], chunk_size_bytes: int) -> None:
    """Try to rebalance an oversized job by moving files to other jobs"""
    if len(oversized_job.files) <= 1:
        return
    
    # Sort files in the oversized job by size (smallest first for moving)
    oversized_job.files.sort(key=lambda x: x.size)
    
    # Try to move smaller files to other jobs
    files_to_move = []
    for file_info in oversized_job.files[:]:  # Copy list to avoid modification during iteration
        if oversized_job.total_size - file_info.size < chunk_size_bytes:
            break
            
        # Find a job that can accommodate this file
        for job in all_jobs:
            if (job != oversized_job and 
                job.total_size + file_info.size <= chunk_size_bytes):
                
                # Move the file
                oversized_job.files.remove(file_info)
                oversized_job.total_size -= file_info.size
                job.files.append(file_info)
                job.total_size += file_info.size
                break
        else:
            # No job can accommodate this file, stop trying
            break


def estimate_file_sizes(files: List[FileInfo]) -> List[FileInfo]:
    """Estimate file sizes for remote transfers where size is unknown"""
    if not files:
        return files
    
    # If we have some files with known sizes, use them to estimate others
    known_sizes = [f.size for f in files if f.size > 0]
    if known_sizes:
        avg_size = sum(known_sizes) // len(known_sizes)
        
        # Update files with unknown sizes
        for file_info in files:
            if file_info.size == 0:
                # Estimate based on file extension or use average
                if file_info.relative_path.endswith(('.txt', '.log', '.conf')):
                    file_info.size = min(avg_size, 1024 * 1024)  # Max 1MB for text files
                elif file_info.relative_path.endswith(('.jpg', '.png', '.gif')):
                    file_info.size = min(avg_size * 2, 10 * 1024 * 1024)  # Max 10MB for images
                elif file_info.relative_path.endswith(('.mp4', '.avi', '.mkv')):
                    file_info.size = avg_size * 10  # Videos are typically larger
                else:
                    file_info.size = avg_size
    else:
        # No known sizes, use conservative estimates
        default_size = 1024 * 1024  # 1MB default
        for file_info in files:
            if file_info.size == 0:
                file_info.size = default_size
    
    return files
