from conans import ConanFile, CMake, tools


class PactMatchingFfiConan(ConanFile):
    name = "pact_matching_ffi"
    version = "0.1.0"
    license = "MITRE Proprietary"
    description = "Wrap of Pact Rust matching logic for C++ use"
    settings = "os", "compiler", "build_type", "arch"
    options = {"shared": [True, False]}
    default_options = {"shared": False}
    generators = "cmake"

    def configure(self):
        self.settings.compiler["gcc"].version = "8"
        self.settings.compiler["gcc"].libcxx = "libstdc++"



    def source(self):
        self.run("git clone -b pact_matching_ffi_conan --single-branch https://github.com/mitre/pact-reference.git")
        #self.run("git clone https://github.com/conan-io/hello.git")
        # This small hack might be useful to guarantee proper /MT /MD linkage
        # in MSVC if the packaged project doesn't have variables to set it
        # properly
   #     tools.replace_in_file("hello/CMakeLists.txt", "PROJECT(HelloWorld)",
    #                          '''PROJECT(HelloWorld)
     #   include(${CMAKE_BINARY_DIR}/conanbuildinfo.cmake)
      #  conan_basic_setup()''')

    def build(self):
        cmake = CMake(self)
        cmake.configure(source_folder="pact-reference/rust/pact_matching_ffi")
        self.run("make generate_header")
        cmake.build()
        self.run("mkdir install");
        self.run("cmake --install . --prefix ./install");

        # Explicit way:
        # self.run('cmake %s/hello %s'
        #          % (self.source_folder, cmake.command_line))
        # self.run("cmake --build . %s" % cmake.build_config)

    def package(self):
        self.copy("*.h", "include", "./install/include")
        self.copy("*.so", "lib", "./install/lib")
        self.copy("*.cmake", "lib", "./install/lib")
 
    def package_info(self):
        self.cpp_info.libs = ["pact_matching_ffi"]

