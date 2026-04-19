# Red Bear OS CMake Toolchain File
# Target: x86_64-unknown-redox
#
# This toolchain file cross-compiles CMake projects (Qt6, etc.) for Red Bear OS.
#
# Required environment variables (set by cookbook or manually):
#   COOKBOOK_SYSROOT       - Target sysroot with built dependencies (headers + libs)
#                            Inside cookbook: per-recipe staging sysroot
#                            Standalone:      point to prefix/x86_64-unknown-redox/sysroot
#
#   COOKBOOK_HOST_SYSROOT  - Host toolchain root containing cross-compiler binaries
#                            Inside cookbook: prefix/x86_64-unknown-redox/sysroot
#                            Standalone:      same path
#
# Usage:
#   cmake -DCMAKE_TOOLCHAIN_FILE=local/recipes/qt/redox-toolchain.cmake ...
#
# References:
#   mk/prefix.mk       - How the cross-toolchain is assembled (GCC + Clang + relibc)
#   mk/config.mk        - TARGET=x86_64-unknown-redox, GNU_TARGET=x86_64-unknown-redox
#   src/cook/script.rs   - generate_cookbook_cmake_file() for the cookbook's own CMake setup
#   .cargo/config.toml   - linker = "x86_64-unknown-redox-gcc"

if(NOT DEFINED COOKBOOK_HOST_SYSROOT AND DEFINED ENV{COOKBOOK_HOST_SYSROOT})
    set(COOKBOOK_HOST_SYSROOT "$ENV{COOKBOOK_HOST_SYSROOT}")
endif()

if(NOT DEFINED COOKBOOK_SYSROOT AND DEFINED ENV{COOKBOOK_SYSROOT})
    set(COOKBOOK_SYSROOT "$ENV{COOKBOOK_SYSROOT}")
endif()

# --- Target platform ---
# Use CMAKE_SYSTEM_NAME "Linux" so CMake internally sets UNIX=TRUE. This is critical:
# Qt's build system uses CONDITION ... AND UNIX to include Unix-specific source files
# (qwaitcondition_unix.cpp, qthread_unix.cpp, qfilesystemengine_unix.cpp, etc.).
# CMake only sets UNIX=TRUE for recognized POSIX system names — "Redox" is not one.
# Redox IS POSIX-compatible, so Linux's CMake platform module is the correct match.
# The __redox__ compiler macro controls Q_OS_REDOX at the C++ level (not CMake).
set(CMAKE_SYSTEM_NAME Linux)
set(CMAKE_SYSTEM_PROCESSOR x86_64)
set(CMAKE_SYSTEM_VERSION 1)

# Redox userspace currently must not emit CET/IBT entry instructions (endbr64),
# because they trap as invalid opcode in the current runtime stack.
set(CMAKE_C_FLAGS "-fcf-protection=none" CACHE STRING "" FORCE)
set(CMAKE_CXX_FLAGS "-fcf-protection=none" CACHE STRING "" FORCE)
set(CMAKE_C_FLAGS_RELEASE "-fcf-protection=none" CACHE STRING "" FORCE)
set(CMAKE_CXX_FLAGS_RELEASE "-fcf-protection=none" CACHE STRING "" FORCE)

# Flag for redox.patch: enables REDOX-specific CMake code paths (mkspec, QPA plugin).
# QtPlatformSupport.cmake checks this variable. Set as CACHE INTERNAL so it persists
# across CMake re-configures and is visible in Qt's CMake modules.
set(REDOX 1 CACHE INTERNAL "Building for Redox OS")

# Mark as cross-compilation so CMake does not attempt to run target binaries
set(CMAKE_CROSSCOMPILING TRUE)

# --- Cross-compiler ---
# The build system produces cross-compilers with the GNU_TARGET prefix:
#   x86_64-unknown-redox-gcc, x86_64-unknown-redox-g++, etc.
# These are GCC frontends that link against relibc (Redox's POSIX libc in Rust).
# They live in prefix/x86_64-unknown-redox/sysroot/bin/ which is added to PATH
# by the cookbook before building (see src/cook/cook_build.rs, COOKBOOK_TOOLCHAIN).
#
# COOKBOOK_HOST_SYSROOT is set to $(ROOT)/$(PREFIX_INSTALL) = prefix/x86_64-unknown-redox/sysroot
# by mk/repo.mk and mk/prefix.mk. Fallback to hardcoded path if unset.
if(NOT DEFINED COOKBOOK_HOST_SYSROOT OR COOKBOOK_HOST_SYSROOT STREQUAL "")
    if(EXISTS "$ENV{HOME}/.redoxer/x86_64-unknown-redox/toolchain/bin/x86_64-unknown-redox-gcc")
        set(COOKBOOK_HOST_SYSROOT "$ENV{HOME}/.redoxer/x86_64-unknown-redox/toolchain")
    else()
        set(COOKBOOK_HOST_SYSROOT "/mnt/data/homes/kellito/Builds/rbos/prefix/x86_64-unknown-redox/sysroot")
    endif()
endif()

set(CMAKE_C_COMPILER "${COOKBOOK_HOST_SYSROOT}/bin/x86_64-unknown-redox-gcc")
set(CMAKE_CXX_COMPILER "${COOKBOOK_HOST_SYSROOT}/bin/x86_64-unknown-redox-g++")

# Toolchain utilities — same prefix, same bin/ directory
set(CMAKE_AR "${COOKBOOK_HOST_SYSROOT}/bin/x86_64-unknown-redox-ar")
set(CMAKE_RANLIB "${COOKBOOK_HOST_SYSROOT}/bin/x86_64-unknown-redox-ranlib")
set(CMAKE_STRIP "${COOKBOOK_HOST_SYSROOT}/bin/x86_64-unknown-redox-strip")
set(CMAKE_OBJCOPY "${COOKBOOK_HOST_SYSROOT}/bin/x86_64-unknown-redox-objcopy")
set(CMAKE_OBJDUMP "${COOKBOOK_HOST_SYSROOT}/bin/x86_64-unknown-redox-objdump")

# pkg-config wrapper — lives in bin/ at the repo root, added to PATH by cookbook
# The wrapper (bin/x86_64-unknown-redox-pkg-config) reads COOKBOOK_SYSROOT to set
# PKG_CONFIG_SYSROOT_DIR and PKG_CONFIG_LIBDIR correctly.
set(PKG_CONFIG_EXECUTABLE "x86_64-unknown-redox-pkg-config")

# --- Sysroot ---
# COOKBOOK_SYSROOT is the per-recipe staging area containing headers and libs from
# all dependencies built so far. Structure: include/, lib/, lib/pkgconfig/, share/.
# Set by src/cook/cook_build.rs (cook_build.rs line 425, 451).
set(CMAKE_SYSROOT "${COOKBOOK_SYSROOT}")
set(CMAKE_FIND_ROOT_PATH "${COOKBOOK_SYSROOT}")

# CMake prefix path for find_package() — Qt6 needs this to locate deps in sysroot
set(CMAKE_PREFIX_PATH "${COOKBOOK_SYSROOT}")

# Explicit library and include search paths
set(CMAKE_LIBRARY_PATH "${COOKBOOK_SYSROOT}/lib")
set(CMAKE_INCLUDE_PATH "${COOKBOOK_SYSROOT}/include")

if(DEFINED ENV{COOKBOOK_SYSROOT} AND EXISTS "$ENV{COOKBOOK_SYSROOT}/lib")
    set(_redbear_sysroot_link_flags "-L$ENV{COOKBOOK_SYSROOT}/lib -Wl,-rpath-link,$ENV{COOKBOOK_SYSROOT}/lib")
    set(CMAKE_EXE_LINKER_FLAGS_INIT "${CMAKE_EXE_LINKER_FLAGS_INIT} ${_redbear_sysroot_link_flags}")
    set(CMAKE_SHARED_LINKER_FLAGS_INIT "${CMAKE_SHARED_LINKER_FLAGS_INIT} ${_redbear_sysroot_link_flags}")
    set(CMAKE_MODULE_LINKER_FLAGS_INIT "${CMAKE_MODULE_LINKER_FLAGS_INIT} ${_redbear_sysroot_link_flags}")
    set(CMAKE_EXE_LINKER_FLAGS "${CMAKE_EXE_LINKER_FLAGS} ${_redbear_sysroot_link_flags}" CACHE STRING "" FORCE)
    set(CMAKE_SHARED_LINKER_FLAGS "${CMAKE_SHARED_LINKER_FLAGS} ${_redbear_sysroot_link_flags}" CACHE STRING "" FORCE)
    set(CMAKE_MODULE_LINKER_FLAGS "${CMAKE_MODULE_LINKER_FLAGS} ${_redbear_sysroot_link_flags}" CACHE STRING "" FORCE)
endif()

if(DEFINED ENV{COOKBOOK_SYSROOT} AND EXISTS "$ENV{COOKBOOK_SYSROOT}/lib/libredbear-qt-strtold-compat.so")
    set(CMAKE_EXE_LINKER_FLAGS "${CMAKE_EXE_LINKER_FLAGS} -Wl,--no-as-needed -L$ENV{COOKBOOK_SYSROOT}/lib -lredbear-qt-strtold-compat" CACHE STRING "" FORCE)
    set(CMAKE_SHARED_LINKER_FLAGS "${CMAKE_SHARED_LINKER_FLAGS} -Wl,--no-as-needed -L$ENV{COOKBOOK_SYSROOT}/lib -lredbear-qt-strtold-compat" CACHE STRING "" FORCE)
    set(CMAKE_C_STANDARD_LIBRARIES_INIT "${CMAKE_C_STANDARD_LIBRARIES_INIT} -Wl,--no-as-needed -L$ENV{COOKBOOK_SYSROOT}/lib -lredbear-qt-strtold-compat")
    set(CMAKE_CXX_STANDARD_LIBRARIES_INIT "${CMAKE_CXX_STANDARD_LIBRARIES_INIT} -Wl,--no-as-needed -L$ENV{COOKBOOK_SYSROOT}/lib -lredbear-qt-strtold-compat")
endif()

# Install prefix — matches the cookbook convention (see cookbook_cmake in script.rs)
set(CMAKE_INSTALL_PREFIX "/usr")

# --- Unix-style install paths ---
# CMAKE_SYSTEM_NAME "Redox" is not a built-in CMake platform, so CMake defaults
# to Generic install paths. Override to match FHS layout (Redox follows Unix conventions).
set(CMAKE_INSTALL_BINDIR "bin" CACHE PATH "")
set(CMAKE_INSTALL_LIBDIR "lib" CACHE PATH "")
set(CMAKE_INSTALL_INCLUDEDIR "include" CACHE PATH "")
set(CMAKE_INSTALL_DATADIR "share" CACHE PATH "")

# --- Search behavior ---
# Host tools (cmake, ninja, etc.) come from the host system — NEVER from sysroot.
# Target libraries, headers, and packages come ONLY from the sysroot.
set(CMAKE_FIND_ROOT_PATH_MODE_PROGRAM NEVER)
set(CMAKE_FIND_ROOT_PATH_MODE_LIBRARY ONLY)
set(CMAKE_FIND_ROOT_PATH_MODE_INCLUDE ONLY)
set(CMAKE_FIND_ROOT_PATH_MODE_PACKAGE ONLY)

# --- Compiler test programs ---
# Allow linker-based try_compile checks. CMake will still treat this as a cross build
# and avoid executing target binaries on the host.
set(CMAKE_TRY_COMPILE_TARGET_TYPE EXECUTABLE)

# --- Shared library support ---
# Redox supports shared libraries on x86_64 (see DYNAMIC_INIT in script.rs).
# The cross-compiler emits SONAME via -Wl,-soname, matching the cookbook's setting.
set(CMAKE_SHARED_LIBRARY_SONAME_C_FLAG "-Wl,-soname,")
set(CMAKE_SHARED_LIBRARY_SONAME_CXX_FLAG "-Wl,-soname,")
set(CMAKE_PLATFORM_USES_PATH_WHEN_NO_SONAME 1)

# --- Redox POSIX compatibility shims ---
# relibc's assert.h does not define static_assert (C11 macro). Provide it for C only
# (C++ has it as a keyword — redefining would cause errors in try_compile checks).
set(CMAKE_C_FLAGS "${CMAKE_C_FLAGS} -Dstatic_assert=_Static_assert -DP_ALL=0 -DP_PID=1 -DP_PGID=2 -Dvfork=fork" CACHE STRING "")
set(CMAKE_CXX_FLAGS "${CMAKE_CXX_FLAGS} -DP_ALL=0 -DP_PID=1 -DP_PGID=2 -Dvfork=fork" CACHE STRING "")

# relibc now provides waitid() itself, but qtbase/forkfd still does not reliably pick up the
# P_PID / P_PGID / P_ALL constants through the active cross-build include path, so keep the
# constants forced here until the downstream build proves them redundant.

# --- Qt6 cross-compilation helpers ---
# QT_MKSPECS_DIR: Qt6's QtMkspecHelpers.cmake sets this from QT_SOURCE_TREE,
# but downstream modules need it pointing at staged mkspecs (where redox-g++ is),
# not qtbase's source tree. Only set when mkspecs are already staged — during
# qtbase's own build, they don't exist yet in the sysroot.
if(DEFINED ENV{COOKBOOK_SYSROOT} AND EXISTS "$ENV{COOKBOOK_SYSROOT}/usr/mkspecs/redox-g++")
    set(QT_MKSPECS_DIR "$ENV{COOKBOOK_SYSROOT}/usr/mkspecs")
endif()
