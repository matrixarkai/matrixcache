// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 MatrixArkAI

// Pre-rename spellings of types whose names have since been brought in line with
// the way Rust names things. Every old spelling still resolves to the type it
// always named, so no caller has to change a line.
//
// These are re-exports rather than `type` aliases so that traits and generic
// types need no special handling, and so a reader who follows one lands on the
// real definition. Keeping them in one file means the compatibility surface can
// be read, and eventually retired, in one place.

mod legacy_names {
    // Acronyms are spelled as ordinary words now (`Ssd`, not `SSD`).
    // `ConcurrentSimpleLRUCache` was already an alias of this kind and moved here
    // from `cache_facades.rs` to sit with the rest.
    #![allow(clippy::upper_case_acronyms)]

    pub use crate::{
        BaseLruList as BaseLRUList, ChunkId as ChunkID,
        ConcurrentReplacementSlru as ConcurrentReplacementSLRU,
        ConcurrentSimpleLruCache as ConcurrentSimpleLRUCache,
        DramPmemDataPlacement as DRAMPMEMDataPlacementType, GcCopyCallback as GCCopyCallback,
        GcCopyCallbackMock as GCCopyCallbackMock, GcMode as GCMode, GhostLruList as GhostLRUList,
        GhostLruPopResult as GhostLRUPopResult, IoBufBuffer as IOBufBuffer,
        LogBasedAllocatorGcEventListener as LogBasedAllocatorGCEventListenerApi,
        LogBasedAllocatorGcEventListenerMock as LogBasedAllocatorGCEventListenerMock,
        LogBasedMemoryAllocatorPmem as LogBasedMemoryAllocatorPMem,
        PmemDispatcher as PMemDispatcher,
        PoolBasedMemoryAllocatorPmem as PoolBasedMemoryAllocatorPMem, RdmaCache as RDMACache,
        RdmaResponse as RDMAResponse, RdmaStorageEnginePmem as RdmaStorageEnginePMem,
        RdmaStorageEngineSsd as RdmaStorageEngineSSD, ReplacementFifo as ReplacementFIFO,
        ReplacementSlru as ReplacementSLRU, SimpleLruCache as SimpleLRUCache,
        SsdEngineKind as SSDEngineType, StorageEngineMultiSsd as StorageEngineMultiSSD,
        StorageEnginePmem as StorageEnginePMem, StorageEngineRocksDb as StorageEngineRocksDB,
        StorageEngineSsd as StorageEngineSSD, StorageGcController as StorageGCController,
        ZeroCopySimpleLruCache as ZeroCopySimpleLRUCache,
    };

    // The enums that classify a thing use Rust's `Kind`, not the older `Type`
    // suffix; where the suffix carried nothing at all it is simply gone.
    // `CacheKeyType` was never the type of a `CacheKey` -- it is the `String`
    // the replacement policies index by -- so it is now `PolicyKey`.
    pub use crate::{
        AccessRecordKind as AccessRecordType, AllocatorKind as AllocatorType,
        CacheAccessRecordKind as CacheAccessRecordType, CacheInstanceKind as CacheInstanceType,
        DataKind as DataType, DramPmemDataPlacement as DramPmemDataPlacementType,
        PolicyKey as CacheKeyType, RdmaReplacementPolicyKind as RdmaReplacementPolicyType,
        RdmaStorageEngineKind as RdmaStorageEngineType, RecordState as RecordStateType,
        ReplacementPolicyKind as ReplacementPolicyType, SsdEngineKind as SsdEngineType,
        StorageEngineKind as StorageEngineType, WriteBufferKind as WriteBufferType,
    };

    // The `*Ptr` spellings named a shared-pointer type that has no counterpart
    // here, and three of them aliased a type to itself. `Index` aliased
    // `SsdIndex` and nothing used it.
    pub use crate::{
        AllocatorAddress as AllocatorPtr, RawBuffer as RawBufferPtr,
        SharedCacheExecutor as ExecutorSharedPtr, SsdIndex as Index,
        StringBuffer as StringBufferPtr, StringViewBuffer as StringViewBufferPtr,
    };

    // Trait names agree on a suffix now: `Api` for a component's interface,
    // `Callback`/`Listener` for something the crate calls back into.
    pub use crate::{
        DramPmemL1Cache as L1CacheImplement, L1CacheApi as L1CacheInterface,
        LogBasedAllocatorGcEventListener as LogBasedAllocatorGcEventListenerApi,
    };

    // `MemcachedWrapper` wrapped nothing -- it serves the memcached-shaped
    // surface in process, deliberately without a daemon.
    pub use crate::InProcessMemcachedCache as MemcachedWrapper;
}

pub use legacy_names::*;
