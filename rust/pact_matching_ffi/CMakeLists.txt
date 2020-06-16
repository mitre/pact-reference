#############################################################################
# CMAKE VERSION
#############################################################################

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
# PROJECT DESCRIPTION
#
# This is pulled from the `Cargo.toml` file.
#############################################################################

set(DESCRIPTION_REGEX "description[\\s]*=[\\s]*\"(.+)\"")

file(STRINGS "${CMAKE_CURRENT_SOURCE_DIR}/Cargo.toml" DESCRIPTION_STRING REGEX ${DESCRIPTION_REGEX})

string(REGEX REPLACE ${DESCRIPTION_REGEX} "\\1" DESCRIPTION_STRING "${DESCRIPTION_STRING}")

#############################################################################
# PROJECT DECLARATION
#############################################################################

project(PactMatchingFfi
        VERSION "${VERSION_STRING}"
        DESCRIPTION "${DESCRIPTION_STRING}"
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
# DEFAULT BUILD TYPE
#
# Make release the default build type
#############################################################################

set(default_build_type "Release")
if(NOT CMAKE_BUILD_TYPE AND NOT CMAKE_CONFIGURATION_TYPES)
  message(STATUS "Setting build type to '${default_build_type}' as none was specified.")
  set(CMAKE_BUILD_TYPE "${default_build_type}" CACHE STRING "Choose the type of build." FORCE)
  # Set the possible values of build type
  set_property(CACHE CMAKE_BUILD_TYPE PROPERTY STRINGS "Debug" "Release")
endif()


#############################################################################
# Find Cargo & Cbindgen
#############################################################################

# Uses the finder specified in `cmake/FindCargo.cmake`
find_package(Cargo REQUIRED)

# Uses the finder specified in `cmake/FindCbindgen.cmake`
find_package(Cbindgen REQUIRED)

#############################################################################
# LIBRARY
#
# Defines the target for the `pact_matching_ffi` crate and teaches CMake how
# to run Cargo and where to find the output build artifacts.
#############################################################################

if(CMAKE_BUILD_TYPE STREQUAL "Debug")
    set(CARGO_BUILD_TYPE "")
    set(TARGET_DIR "debug")
else()
    set(CARGO_BUILD_TYPE "--release")
    set(TARGET_DIR "release")
endif()

if(WIN32)
    set(CARGO_OUTPUT_FILE "pact_matching_ffi.dll")
else()
    set(CARGO_OUTPUT_FILE "pact_matching_ffi.a")
endif()

# Add target for the library
add_custom_target(pact_matching_ffi ALL
    COMMENT "Building library file with Cargo"
    COMMAND ${CARGO_EXECUTABLE} build ${CARGO_BUILD_TYPE} --target-dir ${CMAKE_CURRENT_BINARY_DIR}
    VERBATIM
    BYPRODUCTS "${CMAKE_CURRENT_BINARY_DIR}/${TARGET_DIR}/${CARGO_OUTPUT_FILE}"
    WORKING_DIRECTORY ${CMAKE_CURRENT_SOURCE_DIR})

# Set the location for the pact_matching_ffi target
set_target_properties(pact_matching_ffi PROPERTIES LOCATION ${CMAKE_CURRENT_BINARY_DIR})

# Add command for cbindgen to generate the header file
add_custom_command(OUTPUT "${CMAKE_CURRENT_SOURCE_DIR}/include/pact_matching.h"
    COMMAND ${CBINDGEN_EXECUTABLE} --config ${CMAKE_CURRENT_SOURCE_DIR}/cbindgen.toml --crate pact_matching_ffi --output ${CMAKE_CURRENT_SOURCE_DIR}/include/pact_matching.h
    WORKING_DIRECTORY ${CMAKE_CURRENT_SOURCE_DIR})

# Add custom target, set to always run, which generates the header
add_custom_target(generate_header ALL
    DEPENDS "${CMAKE_CURRENT_SOURCE_DIR}/include/pact_matching.h"
    WORKING_DIRECTORY ${CMAKE_CURRENT_SOURCE_DIR})

# Teach CMake to install the archive file built by the pact_matching_ffi target
install(FILES "${CMAKE_CURRENT_BINARY_DIR}/${CARGO_OUTPUT_FILE}" TYPE LIB)

# Teach CMake to install the header file built by the generate_header target
install(FILES ${CMAKE_CURRENT_SOURCE_DIR}/include/pact_matching.h TYPE INCLUDE)
