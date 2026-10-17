/* The C smoke test of the pk_* ABI (PLAN.md M10.4): compiled as C99 with -pedantic-errors -Wall -Werror (MSVC: C11,
 * /W4 /WX) against the committed header and the built library, then run. Exits 0 and prints "ok" if every check holds. */
#include <math.h>
#include <stdio.h>
#include <string.h>

#include "phasekit.h"

#define CHECK(cond)                                                     \
    do {                                                                \
        if (!(cond)) {                                                  \
            fprintf(stderr, "smoke.c:%d: %s failed\n", __LINE__, #cond); \
            return 1;                                                   \
        }                                                               \
    } while (0)

static int32_t props(pk_registry registry, const char *output, const char *n1, double v1, const char *n2, double v2,
                     const char *fluid, double *out) {
    return pk_props_si(registry, output, strlen(output), n1, strlen(n1), v1, n2, strlen(n2), v2, fluid, strlen(fluid),
                       out);
}

int main(void) {
    pk_registry registry = 0;
    pk_fluid water = 0;
    double t = 0.0, tc = 0.0;
    char phase[16], message[256], name[16];
    size_t needed = 0;

    CHECK(pk_registry_new(PK_DATASET_CORRECTED, &registry) == PK_OK && registry != 0);
    CHECK(props(registry, "T", "P", 101325.0, "Q", 0.0, "Water", &t) == PK_OK);
    CHECK(fabs(t - 373.124) < 1e-3);
    CHECK(pk_props1_si(registry, "Tcrit", 5, "Water", 5, &tc) == PK_OK && fabs(tc - 647.096) < 1e-3);
    CHECK(pk_phase_si(registry, "T", 1, 300.0, "P", 1, 101325.0, "Water", 5, phase, sizeof phase, &needed) == PK_OK);
    CHECK(strcmp(phase, "liquid") == 0 && needed == 7);

    CHECK(props(registry, "X", "T", 300.0, "P", 1e5, "Water", &t) == PK_UNKNOWN_KEY);
    CHECK(pk_last_error(message, sizeof message) == strlen("unknown output key `X`") + 1);
    CHECK(strcmp(message, "unknown output key `X`") == 0);

    CHECK(pk_fluid_new(registry, "R718", 4, &water) == PK_OK);
    CHECK(pk_fluid_name(water, name, sizeof name, &needed) == PK_OK && strcmp(name, "Water") == 0);
    {
        const char *keys[2] = {"Hmass", "Q"};
        size_t lens[2] = {5, 1};
        double ts[2] = {300.0, -1.0}, ps[2] = {101325.0, 101325.0}, out[4], h = 0.0;
        int32_t cells[4];
        CHECK(pk_batch(water, keys, lens, 2, "T", 1, ts, "P", 1, ps, 2, out, cells, 4) == PK_OK);
        CHECK(props(registry, "Hmass", "T", 300.0, "P", 101325.0, "Water", &h) == PK_OK);
        CHECK(cells[0] == PK_OK && out[0] == h);
        CHECK(cells[2] == PK_INVALID_INPUT && isnan(out[2]) && cells[3] == PK_INVALID_INPUT && isnan(out[3]));
        CHECK(pk_batch(water, keys, lens, 2, "T", 1, ts, "P", 1, ps, 2, out, cells, 3) == PK_BAD_LENGTH);
    }
    {
        /* Mixture strings (PLAN.md M13.14): v8.0.0's density of R410A at 300 K and 1 bar by name and by composition
         * (fixtures/coolprop-8.0.0/facts/mixtures.csv); the registry's R410A.mix is its Corrected mole fractions, within
         * 1e-12 of v8.0.0's. A handle on one, and its name. */
        const char *mix = "HEOS::R32[0.697615]&R125[0.302385]";
        pk_fluid r410a = 0;
        double d = 0.0;
        CHECK(props(registry, "Dmolar", "T", 300.0, "P", 1e5, "R410A.mix", &d) == PK_OK);
        CHECK(fabs(d / 40.60488486848982 - 1.0) < 1e-9);
        CHECK(props(registry, "Dmolar", "T", 300.0, "P", 1e5, mix, &d) == PK_OK);
        CHECK(fabs(d / 40.60488484312704 - 1.0) < 1e-9);
        CHECK(pk_fluid_new(registry, mix, strlen(mix), &r410a) == PK_OK);
        CHECK(pk_fluid_name(r410a, message, sizeof message, &needed) == PK_OK);
        CHECK(strcmp(message, "R32[0.697615]&R125[0.302385]") == 0);
        CHECK(pk_fluid_free(r410a) == PK_OK);
    }
    CHECK(pk_fluid_free(water) == PK_OK && pk_fluid_free(water) == PK_STALE_HANDLE);
    CHECK(pk_registry_free(registry) == PK_OK && pk_registry_free(registry) == PK_STALE_HANDLE);
    CHECK(props(registry, "T", "P", 101325.0, "Q", 0.0, "Water", &t) == PK_STALE_HANDLE);

    printf("ok\n");
    return 0;
}
