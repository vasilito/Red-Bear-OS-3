#ifndef _LINUX_MODULE_H
#define _LINUX_MODULE_H

#define MODULE_LICENSE(x)
#define MODULE_AUTHOR(x)
#define MODULE_DESCRIPTION(x)
#define MODULE_VERSION(x)
#define MODULE_ALIAS(x)
#define MODULE_DEVICE_TABLE(type, name)

#define module_init(x)
#define module_exit(x)

#define THIS_MODULE ((void *)0)

#define EXPORT_SYMBOL(x)
#define EXPORT_SYMBOL_GPL(x)
#define EXPORT_SYMBOL_NS(x, ns)

#define MODULE_PARM_DESC(name, desc)
#define module_param(name, type, perm)

#define MODULE_INFO(tag, info)

typedef struct {
    int unused;
} module_t;

#endif
