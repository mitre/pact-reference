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
# CONFIG FILES
#
# Set the location of various config files we'll use throughout.
#################################################################################################

# The path to the cargo config file.
set(CARGO_CONFIG_FILE "${CMAKE_CURRENT_SOURCE_DIR}/Cargo.toml")

# The path to the cbindgen config file.
set(CBINDGEN_CONFIG_FILE "${CMAKE_CURRENT_SOURCE_DIR}/cbindgen.toml")

#################################################################################################
# PROJECT NAME
#
# This is pulled from the `Cargo.toml` file.
#################################################################################################

# Regex for 'name = "<name>"'
set(NAME_REGEX "name = \"(.+)\"")

# Read in the line containing the name
file(STRINGS ${CARGO_CONFIG_FILE} NAME_STRING REGEX ${NAME_REGEX})

# Pick out just the name
string(REGEX REPLACE ${NAME_REGEX} "\\1" NAME_STRING "${NAME_STRING}")

#################################################################################################
# PROJECT VERSION
#
# This is pulled from the `Cargo.toml` file.
#################################################################################################

# Regex for 'version = "<version>"'
set(VERSION_REGEX "^version = \"(.+)\"$")

# Read in the line containing the version
file(STRINGS ${CARGO_CONFIG_FILE} VERSION_STRING REGEX ${VERSION_REGEX})

# Pick out just the version
string(REGEX REPLACE ${VERSION_REGEX} "\\1" VERSION_STRING "${VERSION_STRING}")

#################################################################################################
# PROJECT DESCRIPTION
#
# This is pulled from the `Cargo.toml` file.
#################################################################################################

# Regex for 'description = "<description>"'
set(DESCRIPTION_REGEX "description = \"(.+)\"")

# Read in the line containing the description
file(STRINGS ${CARGO_CONFIG_FILE} DESCRIPTION_STRING REGEX ${DESCRIPTION_REGEX})

# Pick out just the description
string(REGEX REPLACE ${DESCRIPTION_REGEX} "\\1" DESCRIPTION_STRING "${DESCRIPTION_STRING}")

#################################################################################################
# PROJECT DECLARATION
#################################################################################################

# Print message indicating we found the crate information.
message("Found crate ${NAME_STRING} (version ${VERSION_STRING}): ${DESCRIPTION_STRING}")

# Define the project for the current file.
project(
    ${NAME_STRING}
    VERSION ${VERSION_STRING}
    DESCRIPTION ${DESCRIPTION_STRING}
    LANGUAGES NONE)

#################################################################################################
# CMAKE UTILITIES
#
# Add CMake utilities for finding Cargo and Cbindgen to the module path.
#################################################################################################

set(CMAKE_MODULE_PATH "${PROJECT_SOURCE_DIR}/cmake" ${CMAKE_MODULE_PATH})

#################################################################################################
# OUT OF SOURCE BUILDS
#
# Require out-of-source builds for this project. It keeps things much simpler
# and cleaner.
#################################################################################################

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

# The default build type is Release.
set(default_build_type "Release")

if(NOT CMAKE_BUILD_TYPE AND NOT CMAKE_CONFIGURATION_TYPES)
    # Tell the user they're getting the default build type.
    message(STATUS "Setting build type to '${default_build_type}' as none was specified.")

    # Cache the build type.
    set(CMAKE_BUILD_TYPE ${default_build_type} CACHE STRING "Choose the type of build." FORCE)

    # Set the possible values of build type
    set_property(CACHE CMAKE_BUILD_TYPE PROPERTY STRINGS "Debug" "Release")
endif()

#################################################################################################
# Find Cargo & Cbindgen
#
# Uses custom finders to locate the cargo and cbindgen executables and identify their versions
#################################################################################################

# Uses the finder specified in `cmake/FindCargo.cmake`
find_package(Cargo REQUIRED)

# Uses the finder specified in `cmake/FindCbindgen.cmake`
find_package(Cbindgen REQUIRED)

#################################################################################################
# VARIABLES
#
# Sets important variables to be used by the custom targets.
#################################################################################################

# Set cargo build type flag, and name of the folder containing the library file,
# based on the configured build type.
if(CMAKE_BUILD_TYPE STREQUAL "Debug")
    set(CARGO_BUILD_TYPE "")
    set(TARGET_TYPE "debug")
else()
    set(CARGO_BUILD_TYPE "--release")
    set(TARGET_TYPE "release")
endif()

# There could be something more generic here using `cargo metadata` output to discover
# the workspace root and get the target dir from that, and in a more general context
# that's what ought to be done, but for now we know where the target dir is, and should
# just reuse it.
set(CARGO_TARGET_DIR "${CMAKE_CURRENT_SOURCE_DIR}/../target")

# The name of the crate itself.
set(CRATE_NAME ${NAME_STRING})

# Set the CARGO_LIBRARY_NAME based on whether we're on Windows
if(WIN32)
    set(CARGO_LIBRARY_NAME "${CRATE_NAME}.dll")
else()
    set(CARGO_LIBRARY_NAME "${CRATE_NAME}.a")
endif()

# The path to the library file.
set(CARGO_LIBRARY_FILE "${CARGO_TARGET_DIR}/${TARGET_TYPE}/${CARGO_LIBRARY_NAME}")

# Name of the header file.
set(CBINDGEN_HEADER_NAME "pact_matching.h")

# Path to the header file.
set(CBINDGEN_HEADER_FILE "${CMAKE_CURRENT_SOURCE_DIR}/include/${CBINDGEN_HEADER_NAME}")

#################################################################################################
# LIBRARY
#
# Defines the target for building the library file.
#################################################################################################

# Defines the cargo command to build the library file.
add_custom_command(
    OUTPUT ${CARGO_LIBRARY_FILE}
    COMMAND
        ${CARGO_EXECUTABLE}
        build
        ${CARGO_BUILD_TYPE}
        --target-dir ${CARGO_TARGET_DIR}
    WORKING_DIRECTORY ${CMAKE_CURRENT_SOURCE_DIR})

# Add target for the library file
add_custom_target(build_library ALL
    COMMENT "Building library file with cargo"
    DEPENDS ${CARGO_LIBRARY_FILE})

# Teach CMake to install the library file built by the pact_matching_ffi target
install(FILES "${CARGO_LIBRARY_FILE}" TYPE LIB)

#################################################################################################
# HEADER
#
# Defines the target for generating the header file.
#################################################################################################

# Defines the cbindgen command to generate the header file.
add_custom_command(
    OUTPUT ${CBINDGEN_HEADER_FILE}
    COMMAND
        ${CBINDGEN_EXECUTABLE}
        --config ${CBINDGEN_CONFIG_FILE}
        --crate ${CRATE_NAME}
        --output ${CBINDGEN_HEADER_FILE}
    WORKING_DIRECTORY ${CMAKE_CURRENT_SOURCE_DIR})

# Add target for the header file
add_custom_target(generate_header ALL
    COMMENT "Generating header file with cbindgen"
    DEPENDS ${CBINDGEN_HEADER_FILE})

# Teach CMake to install the header file built by the generate_header target
install(FILES "${CBINDGEN_HEADER_FILE}" TYPE INCLUDE)
