add_library(KWinQpaPlugin MODULE
    backingstore.cpp
    clipboard.cpp
    eglhelpers.cpp
    eglplatformcontext.cpp
    integration.cpp
    main.cpp
    offscreensurface.cpp
    platformcursor.cpp
    screen.cpp
    swapchain.cpp
    window.cpp
)

set_target_properties(KWinQpaPlugin PROPERTIES
    AUTOMOC ON
    LIBRARY_OUTPUT_DIRECTORY "${CMAKE_BINARY_DIR}/plugins/platforms"
    RUNTIME_OUTPUT_DIRECTORY "${CMAKE_BINARY_DIR}/plugins/platforms"
    OUTPUT_NAME "qwayland-org.kde.kwin.qpa"
)

ecm_qt_declare_logging_category(KWinQpaPlugin
    HEADER logging.h
    IDENTIFIER KWIN_QPA
    CATEGORY_NAME kwin_qpa_plugin
    DEFAULT_SEVERITY Critical
)

target_link_libraries(KWinQpaPlugin PRIVATE
    Qt::Concurrent
    Qt::CorePrivate
    Qt::GuiPrivate
    Freetype::Freetype # Must be after Qt6 platform support libs
    Fontconfig::Fontconfig
    kwin
)

install(TARGETS KWinQpaPlugin DESTINATION plugins/platforms)
