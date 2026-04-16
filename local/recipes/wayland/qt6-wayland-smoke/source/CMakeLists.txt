cmake_minimum_required(VERSION 3.20)
project(qt6-wayland-smoke LANGUAGES CXX)

set(CMAKE_AUTOMOC ON)
set(CMAKE_CXX_STANDARD 20)
set(CMAKE_CXX_STANDARD_REQUIRED ON)

find_package(Qt6 REQUIRED COMPONENTS Gui Core)

qt_add_executable(qt6-wayland-smoke
    main.cpp
)

qt_add_executable(qt6-bootstrap-check
    bootstrap.cpp
)

qt_add_executable(qt6-plugin-check
    plugincheck.cpp
)

target_compile_options(qt6-wayland-smoke PRIVATE -fcf-protection=none)
target_compile_options(qt6-bootstrap-check PRIVATE -fcf-protection=none)
target_compile_options(qt6-plugin-check PRIVATE -fcf-protection=none)

target_link_options(qt6-wayland-smoke PRIVATE -fcf-protection=none)
target_link_options(qt6-bootstrap-check PRIVATE -fcf-protection=none)
target_link_options(qt6-plugin-check PRIVATE -fcf-protection=none)

target_link_libraries(qt6-wayland-smoke PRIVATE
    Qt6::Core
    Qt6::Gui
)

target_link_libraries(qt6-bootstrap-check PRIVATE
    Qt6::Core
    Qt6::Gui
)

target_link_libraries(qt6-plugin-check PRIVATE
    Qt6::Core
)

install(TARGETS qt6-wayland-smoke RUNTIME DESTINATION bin)
install(TARGETS qt6-bootstrap-check RUNTIME DESTINATION bin)
install(TARGETS qt6-plugin-check RUNTIME DESTINATION bin)
install(FILES qt.conf DESTINATION bin)
