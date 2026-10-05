// The C++ CoolProp v8.0.0 baseline (VERIFICATION.md §12, PLAN.md M1.15): 7 workloads x 5 fluids, one core, written as
// the CSV the Rust benches compare against. Grids are SplitMix64 draws (seed 1, the generator of
// phasekit_verify::sample) between bounds printed in each row, so a Rust bench replays the same states. Every grid is
// single phase or saturated by construction (no rejection sampling): (T, rho) above 1.05 Tc, saturation below 0.98 Tc.
//
// Usage: coolprop_baseline <out.csv> <cpu model> <os> <governor> <date>

#include <algorithm>
#include <chrono>
#include <cmath>
#include <cstdint>
#include <cstdio>
#include <fstream>
#include <functional>
#include <memory>
#include <string>
#include <vector>

#include "CoolProp/AbstractState.h"
#include "Backends/Helmholtz/HelmholtzEOSMixtureBackend.h"
#include "CoolProp/CoolProp.h"
#include "CoolProp/DataStructures.h"

namespace {

// SplitMix64, bit for bit the generator of phasekit_verify::sample (golden vector 0x910a2dec89025cc1 for seed 1).
struct SplitMix64 {
    std::uint64_t state;
    std::uint64_t next() {
        state += 0x9e3779b97f4a7c15ULL;
        std::uint64_t z = state;
        z = (z ^ (z >> 30)) * 0xbf58476d1ce4e5b9ULL;
        z = (z ^ (z >> 27)) * 0x94d049bb133111ebULL;
        return z ^ (z >> 31);
    }
    // lo + (hi - lo) * u, u = top 53 bits * 2^-53: the operations SplitMix64::uniform performs.
    double uniform(double lo, double hi) { return lo + (hi - lo) * (static_cast<double>(next() >> 11) * 0x1.0p-53); }
};

struct Range {
    const char* name;
    double lo, hi;
};

// n points of two variables, drawn in turn (x0, y0, x1, y1, ...).
std::vector<std::pair<double, double>> grid(std::size_t n, Range x, Range y) {
    SplitMix64 rng{1};
    std::vector<std::pair<double, double>> points(n);
    for (auto& p : points) {
        p.first = rng.uniform(x.lo, x.hi);
        p.second = rng.uniform(y.lo, y.hi);
    }
    return points;
}

std::string bounds(Range x, Range y) {
    char buf[256];
    std::snprintf(buf, sizeof buf, "%s=U[%.17g;%.17g] %s=U[%.17g;%.17g] seed=1", x.name, x.lo, x.hi, y.name, y.lo, y.hi);
    return buf;
}

// Median, min and max over `repeats` passes of the mean ns per state.
struct Timing {
    double median, min, max;
};

volatile double sink = 0;  // keeps every result live

Timing time_it(std::size_t states, int repeats, const std::function<double(std::size_t)>& one) {
    std::vector<double> ns;
    for (int r = 0; r < repeats; ++r) {
        double acc = 0;
        const auto t0 = std::chrono::steady_clock::now();
        for (std::size_t i = 0; i < states; ++i) acc += one(i);
        const auto t1 = std::chrono::steady_clock::now();
        sink = sink + acc;
        ns.push_back(std::chrono::duration<double, std::nano>(t1 - t0).count() / static_cast<double>(states));
    }
    std::sort(ns.begin(), ns.end());
    return {ns[ns.size() / 2], ns.front(), ns.back()};
}

}  // namespace

int main(int argc, char** argv) {
    if (argc != 6) {
        std::fprintf(stderr, "usage: coolprop_baseline <out.csv> <cpu model> <os> <governor> <date>\n");
        return 2;
    }
    const std::vector<std::string> fluids = {"Water", "Methane", "R134a", "n-Propane", "n-Heptane"};
    const int repeats = 7;
    std::ofstream csv(argv[1]);
    csv << "# fixture: coolprop-baseline/v1\n"
        << "# coolprop: " << CoolProp::get_global_param_string("version") << " git="
        << CoolProp::get_global_param_string("gitrevision") << "\n"
        << "# cpu: " << argv[2] << "\n"
        << "# os: " << argv[3] << "\n"
        << "# governor: " << argv[4] << "\n"
        << "# date: " << argv[5] << "\n"
        << "# compiler: " << __VERSION__ << " (CMAKE_BUILD_TYPE=Release, no -march)\n"
        << "# method: one thread; per row the median, min and max over " << repeats
        << " passes of the mean ns per state\n"
        << "# columns: workload,fluid,states,median_ns,min_ns,max_ns,grid\n";

    for (const auto& fluid : fluids) {
        std::shared_ptr<CoolProp::AbstractState> as(CoolProp::AbstractState::factory("HEOS", fluid));
        auto* heos = dynamic_cast<CoolProp::HelmholtzEOSMixtureBackend*>(as.get());
        const double tc = as->T_critical(), rhoc = as->rhomolar_critical(), tmax = as->Tmax(), ttriple = as->Ttriple();
        const Range t_single{"T", 1.05 * tc, std::min(1.5 * tc, tmax)}, rho_single{"rho", 0.1 * rhoc, 2.0 * rhoc};
        const Range t_sat{"T", ttriple + 0.05 * (tc - ttriple), 0.98 * tc}, q{"Q", 0.0, 1.0};
        const auto single = grid(10000, t_single, rho_single);
        const auto sat = grid(10000, t_sat, q);
        // The PT, PH and by-name grids are the (T, rho) grid's states, their p and h computed once, untimed.
        std::vector<double> p(single.size()), h(single.size()), psat(sat.size());
        for (std::size_t i = 0; i < single.size(); ++i) {
            as->update(CoolProp::DmolarT_INPUTS, single[i].second, single[i].first);
            p[i] = as->p();
            h[i] = as->hmolar();
        }
        for (std::size_t i = 0; i < sat.size(); ++i) {
            as->update(CoolProp::QT_INPUTS, 0, sat[i].first);
            psat[i] = as->p();
        }

        auto row = [&](const char* workload, std::size_t states, const std::string& grid_text, const std::function<double(std::size_t)>& one) {
            const Timing t = time_it(states, repeats, one);
            char buf[160];
            std::snprintf(buf, sizeof buf, "%s,%s,%zu,%.4g,%.4g,%.4g,", workload, fluid.c_str(), states, t.median, t.min, t.max);
            csv << buf << grid_text << "\n";
            std::fprintf(stderr, "%-10s %-10s %10.1f ns\n", workload, fluid.c_str(), t.median);
        };
        const std::string td = bounds(t_single, rho_single), qt = bounds(t_sat, q);
        const std::string derived = td + " (p and h at each state)";
        const std::string pq = qt + " (p = p_sigma(T))";

        // (1) The order-2 alpha^r bundle at (T, rho): the state set directly, no phase determination.
        row("alphar2", 10000, td, [&](std::size_t i) {
            heos->update_DmolarT_direct(single[i].second, single[i].first);
            return heos->alphar() + heos->dalphar_dDelta() + heos->dalphar_dTau() + heos->d2alphar_dDelta2()
                   + heos->d2alphar_dDelta_dTau() + heos->d2alphar_dTau2();
        });
        // (2) Properties at (T, rho): update DmolarT + h + c_p.
        row("dt_h_cp", 10000, td, [&](std::size_t i) {
            as->update(CoolProp::DmolarT_INPUTS, single[i].second, single[i].first);
            return as->hmolar() + as->cpmolar();
        });
        // (3) QT and (4) PQ through the superancillary (ENABLE_SUPERANCILLARIES is on by default).
        row("qt", 10000, qt, [&](std::size_t i) {
            as->update(CoolProp::QT_INPUTS, sat[i].second, sat[i].first);
            return as->p();
        });
        row("pq", 10000, pq, [&](std::size_t i) {
            as->update(CoolProp::PQ_INPUTS, psat[i], sat[i].second);
            return as->T();
        });
        // (5) PT and (6) PH single phase.
        row("pt", 2000, derived, [&](std::size_t i) {
            as->update(CoolProp::PT_INPUTS, p[i], single[i].first);
            return as->rhomolar();
        });
        row("ph", 500, derived, [&](std::size_t i) {
            as->update(CoolProp::HmolarP_INPUTS, h[i], p[i]);
            return as->T();
        });
        // (7) A by-name PropsSI call: name lookup and backend construction included.
        row("propssi", 500, derived, [&](std::size_t i) { return CoolProp::PropsSI("Hmolar", "T", single[i].first, "P", p[i], fluid); });
    }
    std::fprintf(stderr, "checksum %.17g\n", static_cast<double>(sink));
    return 0;
}
