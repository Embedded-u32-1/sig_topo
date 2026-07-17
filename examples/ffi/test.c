/*
 * M31: C end-to-end demo of the signal-topology C-ABI.
 *
 * Loads the order_approval topology, drives it through submit -> approve -> ship,
 * and asserts the final state is "shipped". The topology JSON is embedded
 * inline so the demo has no file-reading dependency.
 *
 * Compile + run (from the repo root):
 *     gcc -I include examples/ffi/test.c -L target/debug -lsignal_topology \
 *         -Wl,-rpath,target/debug -o /tmp/test_ffi_c && /tmp/test_ffi_c
 */

#include <stdio.h>
#include <stdlib.h>
#include <string.h>

#include "signal_topology.h"

/* Embedded copy of examples/order_approval.json (single-signal: order). */
static const char *ORDER_APPROVAL =
    "{"
    "  \"version\": \"0.1\","
    "  \"signals\": ["
    "    {"
    "      \"id\": \"order\","
    "      \"initial_state\": \"draft\","
    "      \"states\": [\"draft\", \"submitted\", \"approved\", \"rejected\", \"shipped\"]"
    "    }"
    "  ],"
    "  \"transitions\": ["
    "    {"
    "      \"signal_id\": \"order\", \"from\": \"draft\", \"event\": \"submit\","
    "      \"to\": \"submitted\","
    "      \"actions\": {"
    "        \"on_exit\": [\"log_draft_exit\"],"
    "        \"on_transition\": [\"validate_order_payload\"],"
    "        \"on_enter\": [\"notify_submitted\"]"
    "      }"
    "    },"
    "    {"
    "      \"signal_id\": \"order\", \"from\": \"submitted\", \"event\": \"approve\","
    "      \"to\": \"approved\","
    "      \"guard\": \"payload.amount > 0 and payload.amount <= 100000\","
    "      \"actions\": {"
    "        \"on_transition\": [\"reserve_inventory\"],"
    "        \"on_enter\": [\"notify_customer_approved\"]"
    "      }"
    "    },"
    "    {"
    "      \"signal_id\": \"order\", \"from\": \"submitted\", \"event\": \"reject\","
    "      \"to\": \"rejected\","
    "      \"actions\": {"
    "        \"on_transition\": [\"release_hold\"],"
    "        \"on_enter\": [\"notify_customer_rejected\"]"
    "      }"
    "    },"
    "    {"
    "      \"signal_id\": \"order\", \"from\": \"approved\", \"event\": \"ship\","
    "      \"to\": \"shipped\","
    "      \"actions\": {"
    "        \"on_transition\": [\"dispatch_order\"],"
    "        \"on_enter\": [\"notify_shipped\"]"
    "      }"
    "    }"
    "  ]"
    "}";

int main(void) {
    int rc = 0;

    engine_t *engine = engine_new(ORDER_APPROVAL);
    if (!engine) {
        fprintf(stderr, "FAIL: engine_new returned NULL\n");
        return 1;
    }

    char *result;

    result = engine_send_event(engine, "{\"signal_id\":\"order\",\"event\":\"submit\"}");
    printf("submit  -> %s\n", result);
    if (!strstr(result, "\"to\":\"submitted\"")) {
        fprintf(stderr, "FAIL: submit did not reach submitted\n");
        rc = 1;
    }
    engine_free_str(result);
    if (rc) { engine_free(engine); return rc; }

    result = engine_send_event(
        engine,
        "{\"signal_id\":\"order\",\"event\":\"approve\",\"payload\":{\"amount\":5000}}");
    printf("approve -> %s\n", result);
    if (!strstr(result, "\"to\":\"approved\"")) {
        fprintf(stderr, "FAIL: approve did not reach approved\n");
        rc = 1;
    }
    engine_free_str(result);
    if (rc) { engine_free(engine); return rc; }

    result = engine_send_event(engine, "{\"signal_id\":\"order\",\"event\":\"ship\"}");
    printf("ship    -> %s\n", result);
    if (!strstr(result, "\"to\":\"shipped\"")) {
        fprintf(stderr, "FAIL: ship did not reach shipped\n");
        rc = 1;
    }
    engine_free_str(result);
    if (rc) { engine_free(engine); return rc; }

    result = engine_get_state(engine, "order");
    printf("state   -> %s\n", result);
    if (!strstr(result, "\"shipped\"")) {
        fprintf(stderr, "FAIL: final state is not shipped (got %s)\n", result);
        rc = 1;
    }
    engine_free_str(result);

    engine_free(engine);

    if (rc == 0) {
        printf("PASS: order reached shipped via C-ABI\n");
    }
    return rc;
}
