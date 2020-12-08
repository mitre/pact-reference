#include <string.h>
#include <stdlib.h>

#include <gtest/gtest.h>

#include <pact_matching.h>

using namespace pact_matching;

// todo: status enum in ffi

int main(int argc, char** argv) {
    ::testing::InitGoogleTest(&argc, argv);
    return RUN_ALL_TESTS();
}

TEST(MessageTests, DoubleDestroy)
{
    logger_init();
    logger_attach_sink("stdout", LevelFilter_Debug);
    logger_apply();

    Message *msg = message_new();
    ASSERT_EQ(message_delete(msg), EXIT_SUCCESS); // success
    ASSERT_EQ(message_delete(msg), EXIT_FAILURE); // os failure
}

TEST(MessageTests, BadErrorGets)
{
    logger_init();
    logger_attach_sink("stdout", LevelFilter_Debug);
    logger_apply();

    Message *msg = message_new();
    ASSERT_EQ(message_delete(msg), EXIT_SUCCESS); // success
    // deliberately induce failure
    ASSERT_EQ(message_delete(msg), EXIT_FAILURE); // os failure

    char null_error_msg[1];
    null_error_msg[0] = '\0';
    // provided buff null ptr
    ASSERT_EQ(get_error_message(null_error_msg, 1), -1);

    char small_error_msg[3];
    ASSERT_EQ(get_error_message(small_error_msg, 1), -2);
}

TEST(MessageTests, MessageFromJson)
{
    const char *json_str = "{\
        \"description\": \"String\",\
        \"providerState\": \"provider state\",\
        \"matchingRules\": {}\
    }";

    Message *msg_json =
        message_new_from_json(0, json_str, PactSpecification_V3);
    ASSERT_NE(msg_json, nullptr);

    const char *bad_json_str =
        "Lorem ipsum dolor sit amet, consectetur adipiscing elit";

    Message *bad_msg_json =
        message_new_from_json(0, bad_json_str, PactSpecification_V3);
    ASSERT_EQ(bad_msg_json, nullptr);

    // todo: more granularity
}

TEST(MessageTests, MessageDescriptions)
{
    logger_init();
    logger_attach_sink("stdout", LevelFilter_Debug);
    logger_apply();

    const char *desc = "This is a message description.";
    Message *msg = message_new();

    const char *out_desc = message_get_description(msg);
    ASSERT_STREQ(out_desc, NULL);

    ASSERT_EQ(message_set_description(msg, desc), EXIT_SUCCESS);
    out_desc = message_get_description(msg);
    ASSERT_STREQ(out_desc, desc);
}

TEST(MessageTests, MessageProviderState)
{
    logger_init();
    logger_attach_sink("stdout", LevelFilter_Debug);
    logger_apply();

    Message *msg = message_new();
    const ProviderState *state = message_get_provider_state(msg, 0);
    ASSERT_EQ(state, nullptr);

    // todo: there should be an test with a proper provider state
}

TEST(MessageTests, MessageMetadata)
{
    logger_init();
    logger_attach_sink("stdout", LevelFilter_Debug);
    logger_apply();

    Message *msg = message_new();
    const char *out_val = message_find_metadata(msg, "foo");
    ASSERT_STREQ(out_val, NULL);

    ASSERT_EQ(message_insert_metadata(msg, "FirstName", "Fred"), 0);
    // Overwrite test
    ASSERT_EQ(message_insert_metadata(msg, "FirstName", "Gordon"), -1);
    out_val = message_find_metadata(msg, "FirstName");
    ASSERT_STREQ(out_val, "Gordon");

    ASSERT_EQ(message_insert_metadata(msg, "LastName", "Feez"), 0);
    ASSERT_EQ(message_insert_metadata(msg, "Address", "111 W. 52nd Street"), 0);

    out_val = message_find_metadata(msg, "LastName");
    ASSERT_STREQ(out_val, "Feez");

    MetadataIterator *iter = message_get_metadata_iter(msg);
    ASSERT_NE(iter, nullptr);

    MetadataPair *pair = nullptr;
    while ((pair = metadata_iter_next(iter)) != nullptr) {
        // todo: do something
        ASSERT_EQ(metadata_pair_delete(pair), 0);
    }

    // todo: cleanup
    ASSERT_EQ(metadata_iter_delete(iter), 0);
}
