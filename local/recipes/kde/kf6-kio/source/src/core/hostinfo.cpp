#include "hostinfo.h"
#include <QHostInfo>
namespace KIO { namespace HostInfo {
KIOCORE_EXPORT QHostInfo lookupHost(const QString &, unsigned long) { return QHostInfo(); }
KIOCORE_EXPORT QHostInfo lookupCachedHostInfoFor(const QString &) { return QHostInfo(); }
void cacheLookup(const QHostInfo &) {}
} }
