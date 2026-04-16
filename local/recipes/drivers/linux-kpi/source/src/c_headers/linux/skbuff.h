#ifndef _LINUX_SKBUFF_H
#define _LINUX_SKBUFF_H

#include "types.h"

struct net_device;

struct sk_buff {
    void *head;
    void *data;
    unsigned int len;
    unsigned int tail;
    unsigned int end;
};

struct sk_buff_head {
    struct sk_buff *next;
    struct sk_buff *prev;
    u32 qlen;
    unsigned char lock;
};

extern struct sk_buff *alloc_skb(unsigned int size, gfp_t gfp_mask);
extern void kfree_skb(struct sk_buff *skb);
extern void skb_reserve(struct sk_buff *skb, unsigned int len);
extern void *skb_put(struct sk_buff *skb, unsigned int len);
extern void *skb_push(struct sk_buff *skb, unsigned int len);
extern void *skb_pull(struct sk_buff *skb, unsigned int len);
extern unsigned int skb_headroom(const struct sk_buff *skb);
extern unsigned int skb_tailroom(const struct sk_buff *skb);
extern void skb_trim(struct sk_buff *skb, unsigned int len);
extern void skb_queue_head_init(struct sk_buff_head *list);
extern void skb_queue_tail(struct sk_buff_head *list, struct sk_buff *newsk);
extern struct sk_buff *skb_dequeue(struct sk_buff_head *list);
extern void skb_queue_purge(struct sk_buff_head *list);
extern struct sk_buff *skb_peek(const struct sk_buff_head *list);
extern u32 skb_queue_len(const struct sk_buff_head *list);
extern int skb_queue_empty(const struct sk_buff_head *list);
extern struct sk_buff *__netdev_alloc_skb(struct net_device *dev, u32 length, gfp_t gfp_mask);
#define dev_alloc_skb(length) __netdev_alloc_skb(NULL, length, GFP_KERNEL)
extern struct sk_buff *skb_copy(const struct sk_buff *src, gfp_t gfp);
extern struct sk_buff *skb_clone(const struct sk_buff *skb, gfp_t gfp);
extern void skb_set_network_header(struct sk_buff *skb, int offset);
extern void skb_reset_mac_header(struct sk_buff *skb);

#endif
