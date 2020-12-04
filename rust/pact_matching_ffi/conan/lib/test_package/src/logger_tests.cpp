#include <gtest/gtest.h>

#include "../../include/pact_matching_ffi.h"

using namespace pact_matching;

TEST(LoggerTests, ApplyFail)
{
    logger_init();
    int status = logger_attach_sink("stdout", LevelFilter_Debug);
    ASSERT_EQ(status, 0);
    logger_apply();
    status = logger_attach_sink("stderr", LevelFilter_Info);
    // todo: ASSERT_EQ with exact status
    ASSERT_NE(status, 0);
}

TEST(LoggerTests, BadFileSpec)
{
    logger_init();
    int status = logger_attach_sink("/tmp/foo.log",
				    LevelFilter_Error);
    ASSERT_EQ(status, -4); // UnknownSinkType
}

TEST(LoggerTests, InvalidFile)
{
    logger_init();
    int status = logger_attach_sink("file /tmp?></foo.log",
				    LevelFilter_Error);
    ASSERT_EQ(status, -6);
}
