#ifndef _LINUX_DMA_MAPPING_H
#define _LINUX_DMA_MAPPING_H

#include "types.h"
#include <stddef.h>

enum dma_data_direction {
    DMA_BIDIRECTIONAL = 0,
    DMA_TO_DEVICE     = 1,
    DMA_FROM_DEVICE   = 2,
    DMA_NONE          = 3,
};

struct dma_pool;

#define DMA_BIT_MASK(n) (((n) == 64) ? ~0ULL : ((1ULL << (n)) - 1))

extern void *dma_alloc_coherent(void *dev, size_t size,
                                dma_addr_t *dma_handle, gfp_t flags);
extern void  dma_free_coherent(void *dev, size_t size,
                               void *vaddr, dma_addr_t dma_handle);

extern dma_addr_t dma_map_single(void *dev, void *ptr, size_t size,
                                 enum dma_data_direction dir);
extern void dma_unmap_single(void *dev, dma_addr_t addr, size_t size,
                             enum dma_data_direction dir);
extern struct dma_pool *dma_pool_create(const char *name, void *dev, size_t size, size_t align, size_t boundary);
extern void dma_pool_destroy(struct dma_pool *pool);
extern void *dma_pool_alloc(struct dma_pool *pool, gfp_t flags, dma_addr_t *handle);
extern void dma_pool_free(struct dma_pool *pool, void *vaddr, dma_addr_t addr);
extern void dma_sync_single_for_cpu(void *dev, dma_addr_t addr, size_t size, enum dma_data_direction dir);
extern void dma_sync_single_for_device(void *dev, dma_addr_t addr, size_t size, enum dma_data_direction dir);
extern dma_addr_t dma_map_page(void *dev, void *page, size_t offset, size_t size, enum dma_data_direction dir);
extern void dma_unmap_page(void *dev, dma_addr_t addr, size_t size, enum dma_data_direction dir);

static inline int dma_mapping_error(void *dev, dma_addr_t addr)
{
    (void)dev;
    (void)addr;
    return 0;
}

extern int  dma_set_mask(void *dev, u64 mask);
extern int  dma_set_coherent_mask(void *dev, u64 mask);

#endif
