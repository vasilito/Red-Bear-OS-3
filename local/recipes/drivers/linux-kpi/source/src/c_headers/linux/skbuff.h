#ifndef _LINUX_SKBUFF_H
#define _LINUX_SKBUFF_H

#include "types.h"

struct sk_buff {
    void *head;
    void *data;
    unsigned int len;
    unsigned int tail;
    unsigned int end;
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

#endif
