#ifndef _LINUX_DMA_MAPPING_H
#define _LINUX_DMA_MAPPING_H

#include <linux/types.h>

enum dma_data_direction {
    DMA_BIDIRECTIONAL = 0,
    DMA_TO_DEVICE     = 1,
    DMA_FROM_DEVICE   = 2,
    DMA_NONE          = 3,
};

#define DMA_BIT_MASK(n) (((n) == 64) ? ~0ULL : ((1ULL << (n)) - 1))

extern void *dma_alloc_coherent(void *dev, size_t size,
                                dma_addr_t *dma_handle, gfp_t flags);
extern void  dma_free_coherent(void *dev, size_t size,
                               void *vaddr, dma_addr_t dma_handle);

extern dma_addr_t dma_map_single(void *dev, void *ptr, size_t size,
                                 enum dma_data_direction dir);
extern void dma_unmap_single(void *dev, dma_addr_t addr, size_t size,
                             enum dma_data_direction dir);

static inline int dma_mapping_error(void *dev, dma_addr_t addr)
{
    (void)dev;
    (void)addr;
    return 0;
}

extern int  dma_set_mask(void *dev, u64 mask);
extern int  dma_set_coherent_mask(void *dev, u64 mask);

#endif
