cmake_minimum_required(VERSION 3.16)

# Build only C++ core, no QML modules
add_library(Kirigami STATIC)
add_library(KF6::Kirigami ALIAS Kirigami)

# Core C++ sources that don't require QML/QtQuick
target_sources(Kirigami PRIVATE
    enums.h
    imagecolors.cpp
    imagecolors.h
    mnemonicattached.cpp
    mnemonicattached.h
    overlayzstackingattached.cpp
    overlayzstackingattached.h
    pagepool.cpp
    pagepool.h
    scenepositionattached.cpp
    scenepositionattached.h
    spellcheckattached.cpp
    spellcheckattached.h
    wheelhandler.cpp
    wheelhandler.h
)

target_include_directories(Kirigami PUBLIC
    $<BUILD_INTERFACE:${CMAKE_CURRENT_SOURCE_DIR}>
    $<BUILD_INTERFACE:${CMAKE_CURRENT_BINARY_DIR}>
)

target_link_libraries(Kirigami PUBLIC
    Qt6::Core
    Qt6::Gui
    PRIVATE
    Qt6::Concurrent
)

ecm_qt_declare_logging_category(Kirigami
    HEADER loggingcategory.h
    IDENTIFIER KirigamiLog
    CATEGORY_NAME kf.kirigami
    DESCRIPTION "Kirigami"
    DEFAULT_SEVERITY Warning
    EXPORT KIRIGAMI
)

set_target_properties(Kirigami PROPERTIES
    VERSION     ${PROJECT_VERSION}
    SOVERSION   6
    EXPORT_NAME "Kirigami"
)

install(TARGETS Kirigami EXPORT KirigamiTargets ${KF_INSTALL_TARGETS_DEFAULT_ARGS})
install(EXPORT KirigamiTargets
    DESTINATION ${KDE_INSTALL_CMAKEPACKAGEDIR}/KF6Kirigami
    FILE KF6KirigamiTargets.cmake
    NAMESPACE KF6
)

ecm_qt_install_logging_categories(
    EXPORT KIRIGAMI
    FILE kirigami.categories
    DESTINATION ${KDE_INSTALL_LOGGINGCATEGORIESDIR}
)
