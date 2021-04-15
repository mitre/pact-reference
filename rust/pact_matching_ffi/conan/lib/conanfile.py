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

    def build(self):
        self.run("rustup override set nightly")
        cmake = CMake(self)
        cmake.configure(source_folder="pact-reference/rust/pact_matching_ffi")
        self.run("make generate_header")
        cmake.build()
        self.run("mkdir install");
        self.run("cmake --install . --prefix ./install");

    def package(self):
        self.copy("*.h", "include", "./install/include")
        self.copy("*.so", "lib", "./install/lib")
        self.copy("*.cmake", "lib", "./install/lib")
 
    def package_info(self):
        self.cpp_info.libs = ["pact_matching_ffi"]

