cmake_minimum_required(VERSION 3.16)
project(vollminputd_panel LANGUAGES CXX)

set(CMAKE_CXX_STANDARD 17)
set(CMAKE_CXX_STANDARD_REQUIRED ON)
find_package(Qt6 REQUIRED COMPONENTS Widgets Network)

add_executable(vollminputd-panel main.cpp)
target_link_libraries(vollminputd-panel PRIVATE Qt6::Widgets Qt6::Network)
install(TARGETS vollminputd-panel RUNTIME DESTINATION bin)
