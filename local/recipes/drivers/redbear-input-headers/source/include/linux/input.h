#if defined(__linux__) || defined(__redox__)
#include "linux/input.h"
#elif __FreeBSD__
#include "freebsd/input.h"
#endif