"""
Rsync Wrapper - Main wrapper class
"""

import os
import subprocess
import time
from concurrent.futures import ThreadPoolExecutor, as_completed
from pathlib import Path
from typing import List, Optional, Tuple

from rich.console import Console
from rich.progress import Progress, TaskID, SpinnerColumn, TextColumn, BarColumn, TimeRemainingColumn
from rich.table import Table

from .bandwidth import BandwidthMonitor
from .jobs import FileInfo, TransferJob, create_balanced_jobs

console = Console()

class RsyncWrapper:
    """rsync wrapper with intelligent parallelization"""
    
    def __init__(
        self, 
        source: str, 
        destination: str, 
        rsync_args: List[str], 
        max_threads: Optional[int] = None, 
        chunk_size_mb: int = 100
    ):
        self.source = source
        self.destination = destination
        self.rsync_args = rsync_args
        self.chunk_size_mb = chunk_size_mb
        self.chunk_size_bytes = chunk_size_mb * 1024 * 1024
        
        # Determine if this is a remote transfer
        self.is_remote = self._is_remote_transfer(source, destination)

        if '-z' not in self.rsync_args and self.is_remote:
            self.rsync_args.append('-z')
        
        # Set optimal thread count
        if max_threads is None:
            if self.is_remote:
                # For remote transfers, limit threads based on bandwidth efficiency
                self.max_threads = min(4, os.cpu_count() or 1)
            else:
                # For local transfers, use more threads
                self.max_threads = min(8, os.cpu_count() or 1)
        else:
            self.max_threads = max_threads
            
        self.bandwidth_monitor = BandwidthMonitor()
        
    def _is_remote_transfer(self, source: str, destination: str) -> bool:
        """Check if transfer involves remote hosts"""
        for path in [source, destination]:
            # Check for SSH-style paths (user@host:path) or rsync URLs
            if '://' in path or (':' in path and not Path(path).exists()):
                return True
        return False
    
    def _get_file_list(self) -> List[FileInfo]:
        """Get list of files to transfer with their sizes"""
        console.print("[blue]Scanning files...[/blue]")
        
        try:
            # Use rsync --dry-run to get file list with sizes
            cmd = [
                'rsync', 
                '--dry-run', 
                '--stats', 
                '--human-readable',
                '--itemize-changes'
            ] + self.rsync_args + [self.source, self.destination]
            
            result = subprocess.run(cmd, capture_output=True, text=True, check=True)
            
            files = []
            base_source = Path(self.source).resolve() if Path(self.source).exists() else self.source
            
            for line in result.stdout.split('\n'):
                if line.startswith('>f'):  # File being transferred
                    # Parse itemized output: >f+++++++++ path/to/file
                    parts = line.split(None, 1)
                    if len(parts) >= 2:
                        file_path = parts[1].strip()
                        
                        # Try to get actual file size
                        try:
                            if isinstance(base_source, Path):
                                full_path = base_source / file_path
                                if full_path.exists():
                                    size = full_path.stat().st_size
                                    files.append(FileInfo(
                                        path=str(full_path),
                                        size=size,
                                        relative_path=file_path
                                    ))
                            else:
                                # For remote sources, estimate size or use 0
                                files.append(FileInfo(
                                    path=file_path,
                                    size=0,  # Will be estimated
                                    relative_path=file_path
                                ))
                        except (OSError, ValueError):
                            continue
            
            # If no files found with itemized changes, fall back to simpler method
            if not files:
                files = self._get_file_list_fallback()
                
            return files
            
        except subprocess.CalledProcessError as e:
            console.print(f"[red]Error scanning files: {e.stderr}[/red]")
            return self._get_file_list_fallback()
    
    def _get_file_list_fallback(self) -> List[FileInfo]:
        """Fallback method to get file list"""
        files = []
        
        if Path(self.source).exists():
            source_path = Path(self.source)
            if source_path.is_file():
                files.append(FileInfo(
                    path=str(source_path),
                    size=source_path.stat().st_size,
                    relative_path=source_path.name
                ))
            else:
                # Walk directory
                for root, dirs, filenames in os.walk(source_path):
                    for filename in filenames:
                        file_path = Path(root) / filename
                        try:
                            size = file_path.stat().st_size
                            rel_path = file_path.relative_to(source_path)
                            files.append(FileInfo(
                                path=str(file_path),
                                size=size,
                                relative_path=str(rel_path)
                            ))
                        except (OSError, ValueError):
                            continue
        
        return files
    
    def _execute_job(self, job: TransferJob, progress: Progress, task_id: TaskID) -> Tuple[bool, str]:
        """Execute a single transfer job"""
        if not job.files:
            return True, "No files to transfer"
        
        # Create temporary file list
        import tempfile
        with tempfile.NamedTemporaryFile(mode='w', delete=False, suffix='.txt') as f:
            for file_info in job.files:
                f.write(f"{file_info.relative_path}\n")
            temp_file = f.name
        
        try:
            # Build rsync command with files-from
            cmd = [
                'rsync',
                f'--files-from={temp_file}'
            ] + self.rsync_args + [self.source, self.destination]
            
            # Execute rsync
            start_time = time.time()
            result = subprocess.run(
                cmd,
                capture_output=True,
                text=True,
                check=False
            )
            
            duration = time.time() - start_time
            
            # Update progress
            progress.update(task_id, advance=job.total_size)
            
            if result.returncode == 0:
                return True, f"Transferred {len(job.files)} files ({job.total_size / (1024**2):.1f} MB) in {duration:.1f}s"
            else:
                error_msg = result.stderr.strip() if result.stderr else "Unknown error"
                return False, f"Error: {error_msg}"
                
        except Exception as e:
            return False, f"Exception: {str(e)}"
        finally:
            # Clean up temporary file
            try:
                os.unlink(temp_file)
            except OSError:
                pass
    
    def run(self) -> bool:
        """Execute the smart rsync transfer"""
        console.print(f"[green]Rsync Wrapper v0.1.0[/green]")
        console.print(f"Source: {self.source}")
        console.print(f"Destination: {self.destination}")
        console.print(f"Remote transfer: {self.is_remote}")
        console.print(f"Max threads: {self.max_threads}")
        console.print(f"Chunk size: {self.chunk_size_mb} MB")
        
        # Get file list
        files = self._get_file_list()
        if not files:
            console.print("[yellow]No files found to transfer[/yellow]")
            return True
            
        total_size = sum(f.size for f in files)
        console.print(f"Found {len(files)} files, total size: {total_size / (1024**2):.1f} MB")
        
        # Create balanced jobs
        jobs = create_balanced_jobs(files, self.max_threads, self.chunk_size_bytes)
        console.print(f"Created {len(jobs)} transfer jobs")
        
        # Show job distribution
        if len(jobs) > 1:
            table = Table(title="Job Distribution")
            table.add_column("Job ID", style="cyan")
            table.add_column("Files", style="magenta")
            table.add_column("Size (MB)", style="green")
            table.add_column("Avg Size (KB)", style="yellow")
            
            for job in jobs:
                avg_size = (job.total_size / len(job.files)) / 1024 if job.files else 0
                table.add_row(
                    str(job.job_id),
                    str(len(job.files)),
                    f"{job.total_size / (1024**2):.1f}",
                    f"{avg_size:.1f}"
                )
            
            console.print(table)
        
        # Execute jobs with progress tracking
        with Progress(
            SpinnerColumn(),
            TextColumn("[progress.description]{task.description}"),
            BarColumn(),
            TextColumn("[progress.percentage]{task.percentage:>3.0f}%"),
            TimeRemainingColumn(),
            console=console,
            transient=False
        ) as progress:
            
            # Create progress tasks
            tasks = {}
            for job in jobs:
                task_id = progress.add_task(f"Job {job.job_id}", total=job.total_size)
                tasks[job.job_id] = task_id
            
            # Execute jobs
            success_count = 0
            start_time = time.time()
            
            with ThreadPoolExecutor(max_workers=self.max_threads) as executor:
                future_to_job = {
                    executor.submit(self._execute_job, job, progress, tasks[job.job_id]): job 
                    for job in jobs
                }
                
                for future in as_completed(future_to_job):
                    job = future_to_job[future]
                    try:
                        success, message = future.result()
                        if success:
                            success_count += 1
                            console.print(f"[green]✓ Job {job.job_id}: {message}[/green]")
                        else:
                            console.print(f"[red]✗ Job {job.job_id}: {message}[/red]")
                    except Exception as exc:
                        console.print(f"[red]✗ Job {job.job_id} generated an exception: {exc}[/red]")
            
            total_time = time.time() - start_time
            
        success_rate = success_count / len(jobs) if jobs else 0
        avg_speed = (total_size / total_time) / (1024**2) if total_time > 0 else 0
        
        console.print(f"\n[bold]Transfer completed in {total_time:.1f}s[/bold]")
        console.print(f"Success rate: {success_count}/{len(jobs)} jobs ({success_rate:.1%})")
        console.print(f"Average speed: {avg_speed:.1f} MB/s")
        
        return success_count == len(jobs)
