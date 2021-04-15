#################################################################################################
# CMAKE VERSION
#################################################################################################

# Set the minimum to 3.15. This is arbitrary and we should probably try to
# test everything with older CMake versions once this is all written, to
# figure out an actual lower-bound.
cmake_minimum_required(VERSION 3.15...3.17)

# Set policies appropriately, so it knows when to warn about policy
# violations.
if(${CMAKE_VERSION} VERSION_LESS 3.17)
    cmake_policy(VERSION ${CMAKE_MAJOR_VERSION}.${CMAKE_MINOR_VERSION})
else()
    cmake_policy(VERSION 3.17)
endif()

#################################################################################################
# PROJECT DECLARATION
#############################################################
project(C_EXAMPLE_1
        VERSION "0.1.0"
        DESCRIPTION "A basic example of a C consumer for the pact matching FFI"
        LANGUAGES C CXX)
set (CMAKE_CXX_STANDARD 11)
enable_testing()


#################################################################################################
# OUT OF SOURCE BUILDS
#
# Require out-of-source builds for this project. It keeps things much simpler
# and cleaner.
#################################################################################################

INCLUDE(${CMAKE_BINARY_DIR}/conanbuildinfo.cmake)
conan_basic_setup()

# Set a path to the CMake config (this file)
file(TO_CMAKE_PATH "${PROJECT_BINARY_DIR}/CMakeLists.txt" LOC_PATH)

# Define the error message to potentially be printed.
set(OOS_MSG "\
You cannot build in a source directory (or any directory with a CMakeLists.txt file). \
Please make a build subdirectory. \
Feel free to remove CMakeCache.txt and CMakeFiles.
")

# If that file path exists, we're doing an in-source build, so we should exit with a fatal
# error complaining only out-of-source builds are supported.
if(EXISTS ${LOC_PATH})
    message(FATAL_ERROR ${OOS_MSG})
endif()

#################################################################################################
# DEFAULT BUILD TYPE
#
# Make release the default build type
#################################################################################################

set(default_build_type "Release")
if(NOT CMAKE_BUILD_TYPE AND NOT CMAKE_CONFIGURATION_TYPES)
  message(STATUS "Setting build type to '${default_build_type}' as none was specified.")
  set(CMAKE_BUILD_TYPE "${default_build_type}" CACHE STRING "Choose the type of build." FORCE)
  # Set the possible values of build type
  set_property(CACHE CMAKE_BUILD_TYPE PROPERTY STRINGS "Debug" "Release")
endif()

#################################################################################################
# FIND PACT MATCHING FFI
#
# This ensures CMake can find the pact matching FFI library file
#################################################################################################

# Sets the search path to the location of the package config
set(SEARCH_PATH "${PROJECT_BINARY_DIR}/lib/cmake")

# Find the pact matching FFI package and load the imported target
find_package(PactMatchingFfi REQUIRED CONFIG PATHS ${SEARCH_PATH})

#################################################################################################
# BUILD
#################################################################################################


link_directories("${PROJECT_BINARY_DIR}/lib")


# Define the executable
add_executable(example src/main.c)
add_executable(PactFFITests src/logger_tests.cpp src/message_tests.cpp)
ADD_TEST(NAME stuff
         WORKING_DIRECTORY ${CMAKE_BINARY_DIR}/bin
         COMMAND ${PROJECT_NAME})

# Link to pact matching FFI

target_include_directories(PactFFITests PRIVATE "include")

target_link_libraries(example PRIVATE pact_matching_ffi)
target_link_libraries(PactFFITests PRIVATE pact_matching_ffi)
target_link_libraries(PactFFITests PRIVATE gtest pthread)


