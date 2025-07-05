# src/smart_rsync/__init__.py
"""
parsync - rsync wrapper with optimized parallelization
"""

__version__ = "0.1.0"
__author__ = "Sapherey"
__email__ = "adarshdas950@gmail.com"

from .wrapper import RsyncWrapper
from .jobs import FileInfo, TransferJob, create_balanced_jobs
from .bandwidth import BandwidthMonitor

__all__ = [
    "RsyncWrapper",
    "FileInfo", 
    "TransferJob",
    "create_balanced_jobs",
    "BandwidthMonitor",
]
