#include <QByteArray>
#include <QGuiApplication>

#include <array>
#include <cstdio>

static void dumpPluginElfHeader(const char *path) {
    std::fprintf(stderr, "qt6-bootstrap-check inspecting %s\n", path);
    FILE *file = std::fopen(path, "rb");
    if (!file) {
        std::perror("fopen");
        return;
    }

    std::array<unsigned char, 64> hdr{};
    const size_t n = std::fread(hdr.data(), 1, hdr.size(), file);
    std::fclose(file);

    std::fprintf(stderr, "qt6-bootstrap-check read %zu bytes\n", n);
    std::fprintf(stderr, "qt6-bootstrap-check ELF header bytes:");
    for (size_t i = 0; i < n; ++i) {
        std::fprintf(stderr, " %02x", hdr[i]);
    }
    std::fprintf(stderr, "\n");

    if (n >= 58) {
        const unsigned phentsize = unsigned(hdr[54]) | (unsigned(hdr[55]) << 8);
        const unsigned phnum = unsigned(hdr[56]) | (unsigned(hdr[57]) << 8);
        std::fprintf(stderr,
                     "qt6-bootstrap-check decoded ELF phentsize=%u phnum=%u\n",
                     phentsize,
                     phnum);
    }
}

int main(int argc, char **argv) {
    const QByteArray platform = qEnvironmentVariableIsSet("QT_QPA_PLATFORM")
            ? qgetenv("QT_QPA_PLATFORM")
            : QByteArray("minimal");

    qputenv("QT_QPA_PLATFORM", platform);
    std::fprintf(stderr, "qt6-bootstrap-check before QGuiApplication platform=%s\n", platform.constData());
    dumpPluginElfHeader("/usr/plugins/platforms/libqminimal.so");
    QGuiApplication app(argc, argv);
    std::fprintf(stderr, "qt6-bootstrap-check after QGuiApplication platform=%s\n", platform.constData());
    return 0;
}
