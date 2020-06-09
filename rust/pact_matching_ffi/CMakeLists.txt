#############################################################################
# CMAKE VERSION
#############################################################################

# Set the minimum to 3.10. This is arbitrary and we should probably try to
# test everything with older CMake versions once this is all written, to
# figure out an actual lower-bound.
cmake_minimum_required(VERSION 3.10...3.17)

# Set policies appropriately, so it knows when to warn about policy
# violations.
if(${CMAKE_VERSION} VERSION_LESS 3.17)
    cmake_policy(VERSION ${CMAKE_MAJOR_VERSION}.${CMAKE_MINOR_VERSION})
else()
    cmake_policy(VERSION 3.17)
endif()

#############################################################################
# PROJECT VERSION
#
# This is pulled from the `Cargo.toml` file.
#############################################################################

# Regex for 'version = "<version>"'
set(VERSION_REGEX "version[\\s]*=[\\s]*\"(.+)\"")

# Read in the line containing the version
file(STRINGS "${CMAKE_CURRENT_SOURCE_DIR}/Cargo.toml" VERSION_STRING REGEX ${VERSION_REGEX})

# Pick out just the version
string(REGEX REPLACE ${VERSION_REGEX} "\\1" VERSION_STRING "${VERSION_STRING}")

#############################################################################
# PROJECT DECLARATION
#############################################################################

project(PactMatchingFfi
        VERSION "${VERSION_STRING}"
        DESCRIPTION "Foreign-function interface for pact_matching"
        LANGUAGES NONE)

#############################################################################
# CMAKE UTILITIES
#
# Add CMake utilities for finding Cargo and Cbindgen to the module path.
#############################################################################

set(CMAKE_MODULE_PATH "${PROJECT_SOURCE_DIR}/cmake" ${CMAKE_MODULE_PATH})

#############################################################################
# OUT OF SOURCE BUILDS
#
# Require out-of-source builds for this project. It keeps things much simpler
# and cleaner.
#############################################################################

file(TO_CMAKE_PATH "${PROJECT_BINARY_DIR}/CMakeLists.txt" LOC_PATH)
if(EXISTS "${LOC_PATH}")
    message(FATAL_ERROR "You cannot build in a source directory (or any directory with a CMakeLists.txt file). Please make a build subdirectory. Feel free to remove CMakeCache.txt and CMakeFiles.")
endif()

#############################################################################
# LIBRARY
#
# Require out-of-source builds for this project. It keeps things much simpler
# and cleaner.
#############################################################################

# TODO: Add code to find Cargo and find Cbindgen

if (CMAKE_BUILD_TYPE STREQUAL "Debug")
    set(CARGO_CMD cargo build)
    set(TARGET_DIR "debug")
else ()
    set(CARGO_CMD cargo build --release)
    set(TARGET_DIR "release")
endif ()

add_custom_target(pact_matching_ffi ALL
    # Run the Cargo build.
    COMMAND CARGO_TARGET_DIR=${CMAKE_CURRENT_BINARY_DIR} ${CARGO_CMD} 
    # TODO: Add cbindgen call to generate the header.
    WORKING_DIRECTORY ${CMAKE_CURRENT_SOURCE_DIR})
set_target_properties(pact_matching_ffi PROPERTIES LOCATION ${CMAKE_CURRENT_BINARY_DIR})

# We don't need the include directory, but users of this library will.
target_include_directories(pact_matching_ffi INTERFACE include)

# TODO: Finish this file https://cliutils.gitlab.io/modern-cmake/chapters/basics/programs.html

# Reference: https://michael-f-bryan.github.io/rust-ffi-guide/setting_up.html
