cmake_minimum_required(VERSION 3.21)

project(citac_epub_qt_prototype LANGUAGES CXX)

set(CMAKE_CXX_STANDARD 20)
set(CMAKE_CXX_STANDARD_REQUIRED ON)
set(CMAKE_CXX_EXTENSIONS OFF)

find_package(Qt6 6.5 REQUIRED COMPONENTS Core Gui)

qt_add_executable(citac-epub-qt-prototype
    main.cpp
)

target_link_libraries(citac-epub-qt-prototype PRIVATE
    Qt6::Core
    Qt6::Gui
)

target_compile_options(citac-epub-qt-prototype PRIVATE
    $<$<CXX_COMPILER_ID:GNU,Clang>:-Wall -Wextra -Wpedantic>
)
