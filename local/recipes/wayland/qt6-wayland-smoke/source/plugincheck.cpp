#include <QCoreApplication>
#include <QDebug>
#include <QFile>
#include <QPluginLoader>

#include <elf.h>
#include <cstddef>

#include <cstdio>
#include <fstream>

static constexpr const char *PhaseFile = "/home/root/.qt6-plugin-check.phase";

static void mark(const char *value) {
    std::ofstream out(PhaseFile, std::ios::trunc);
    out << value << '\n';
    out.flush();
}

int main(int argc, char **argv) {
    mark("before-qcoreapplication");
    std::fprintf(stderr, "qt6-plugin-check before QCoreApplication\n");
    std::fflush(stderr);
    QCoreApplication app(argc, argv);
    mark("after-qcoreapplication");
    std::fprintf(stderr, "qt6-plugin-check after QCoreApplication\n");
    std::fflush(stderr);

    const QString plugin = argc > 1
            ? QString::fromLocal8Bit(argv[1])
            : QStringLiteral("/usr/plugins/platforms/libqminimal.so");

    QFile rawFile(plugin);
    if (rawFile.open(QIODevice::ReadOnly)) {
        const QByteArray header = rawFile.read(64);
        qInfo() << "qt6-plugin-check raw-header" << header.toHex(' ');
        qInfo() << "qt6-plugin-check sizeof(Elf64_Word)" << sizeof(Elf64_Word);
        qInfo() << "qt6-plugin-check sizeof(Elf64_Ehdr)" << sizeof(Elf64_Ehdr);
        qInfo() << "qt6-plugin-check offsetof(e_phentsize)" << offsetof(Elf64_Ehdr, e_phentsize);
        if (header.size() >= 56) {
            const quint8 low = static_cast<quint8>(header[54]);
            const quint8 high = static_cast<quint8>(header[55]);
            const quint16 phentsize = quint16(low) | (quint16(high) << 8);
            qInfo() << "qt6-plugin-check raw-e_phentsize" << phentsize;
            const auto *elfHeader = reinterpret_cast<const Elf64_Ehdr *>(header.constData());
            qInfo() << "qt6-plugin-check struct-e_phentsize" << elfHeader->e_phentsize;
        }
    } else {
        qWarning() << "qt6-plugin-check failed to open raw file" << rawFile.errorString();
    }

    QPluginLoader loader(plugin);
    mark("before-metadata");
    std::fprintf(stderr, "qt6-plugin-check before metadata\n");
    std::fflush(stderr);
    qInfo() << "qt6-plugin-check file" << plugin;
    qInfo() << "qt6-plugin-check metaData" << loader.metaData();

    mark("before-load");
    std::fprintf(stderr, "qt6-plugin-check before load\n");
    std::fflush(stderr);
    if (!loader.load()) {
        mark("load-failed");
        qWarning() << "qt6-plugin-check load failed" << loader.errorString();
        return 1;
    }

    QObject *instance = loader.instance();
    if (!instance) {
        mark("instance-failed");
        qWarning() << "qt6-plugin-check instance failed" << loader.errorString();
        return 2;
    }

    mark("instance-ok");
    std::fprintf(stderr, "qt6-plugin-check instance ok\n");
    std::fflush(stderr);
    qInfo() << "qt6-plugin-check instance ok" << instance->metaObject()->className();
    return 0;
}
