from conans import ConanFile, CMake
import os

class HelloTestConan(ConanFile):
    settings = "os", "compiler", "build_type", "arch"
#    generators = "cmake"

    def configure(self):
        self.settings.compiler["gcc"].version = "8"
        self.settings.compiler["gcc"].libcxx = "libstdc++"

    def build(self):
        cmake = CMake(self)
        cmake.configure()
        cmake.build()

    def imports(self):
        self.copy("*.h", "include", "include")
        self.copy("*.so", "lib", "lib")
        self.copy("*.cmake", "lib", "lib")

    def test(self):
        os.chdir("bin")
        self.run(".%sexample" % os.sep)
