cmake_minimum_required(VERSION 3.20)
project(redbear-greeter-ui LANGUAGES CXX)

set(CMAKE_AUTOMOC ON)
set(CMAKE_AUTORCC ON)
set(CMAKE_CXX_STANDARD 20)
set(CMAKE_CXX_STANDARD_REQUIRED ON)

find_package(Qt6 REQUIRED COMPONENTS Core Gui Qml Quick QuickControls2)

qt_add_executable(redbear-greeter-ui
    main.cpp
    greeter_backend.cpp
    greeter_backend.h
    resources.qrc
)

target_compile_options(redbear-greeter-ui PRIVATE -fcf-protection=none)
target_link_options(redbear-greeter-ui PRIVATE -fcf-protection=none)

target_link_libraries(redbear-greeter-ui PRIVATE
    Qt6::Core
    Qt6::Gui
    Qt6::Qml
    Qt6::Quick
    Qt6::QuickControls2
)

install(TARGETS redbear-greeter-ui RUNTIME DESTINATION bin)
