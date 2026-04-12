#ifndef _LINUX_PRINTK_H
#define _LINUX_PRINTK_H

#include <stdio.h>

#define KERN_SOH     "\001"
#define KERN_EMERG   KERN_SOH "0"
#define KERN_ALERT   KERN_SOH "1"
#define KERN_CRIT    KERN_SOH "2"
#define KERN_ERR     KERN_SOH "3"
#define KERN_WARNING KERN_SOH "4"
#define KERN_NOTICE  KERN_SOH "5"
#define KERN_INFO    KERN_SOH "6"
#define KERN_DEBUG   KERN_SOH "7"
#define KERN_DEFAULT KERN_SOH "d"

#define pr_info(fmt, ...) \
    fprintf(stdout, "[INFO] " fmt "\n", ##__VA_ARGS__)

#define pr_warn(fmt, ...) \
    fprintf(stderr, "[WARN] " fmt "\n", ##__VA_ARGS__)

#define pr_err(fmt, ...) \
    fprintf(stderr, "[ERR]  " fmt "\n", ##__VA_ARGS__)

#define pr_debug(fmt, ...) \
    ((void)0)

#define pr_emerg(fmt, ...) \
    fprintf(stderr, "[EMERG] " fmt "\n", ##__VA_ARGS__)

#define pr_alert(fmt, ...) \
    fprintf(stderr, "[ALERT] " fmt "\n", ##__VA_ARGS__)

#define pr_crit(fmt, ...) \
    fprintf(stderr, "[CRIT] " fmt "\n", ##__VA_ARGS__)

#define pr_notice(fmt, ...) \
    fprintf(stdout, "[NOTE] " fmt "\n", ##__VA_ARGS__)

#define printk(fmt, ...) \
    fprintf(stdout, fmt, ##__VA_ARGS__)

#define dev_info(dev, fmt, ...) \
    pr_info(fmt, ##__VA_ARGS__)

#define dev_warn(dev, fmt, ...) \
    pr_warn(fmt, ##__VA_ARGS__)

#define dev_err(dev, fmt, ...) \
    pr_err(fmt, ##__VA_ARGS__)

#define dev_dbg(dev, fmt, ...) \
    pr_debug(fmt, ##__VA_ARGS__)

#endif
