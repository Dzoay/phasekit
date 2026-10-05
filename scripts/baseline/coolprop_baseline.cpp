// The C++ CoolProp v8.0.0 baseline (VERIFICATION.md §12, PLAN.md M1.15): 7 workloads x 5 fluids, one core, written as
// the CSV the Rust benches compare against. Grids are SplitMix64 draws (seed 1, the generator of
// phasekit_verify::sample) between bounds printed in each row, so a Rust bench replays the same states. Every grid is
// single phase or saturated by construction (no rejection sampling): (T, rho) above 1.05 Tc, saturation below 0.98 Tc.
//
// Beside the timing CSV it writes <out>.memory.csv (heap and resident bytes: the library's first use, per state, every
// fluid loaded) and <out>.scaling.csv (throughput at 1, 2, 4, 6 and 12 threads, one state per thread and fluid).
// Linux only: glibc's mallinfo2, /proc/self/statm and sched_setaffinity.
//
// Usage: coolprop_baseline <out.csv> <cpu model> <os> <governor> <date>

#include <malloc.h>
#include <sched.h>
#include <unistd.h>

#include <algorithm>
#include <atomic>
#include <chrono>
#include <cmath>
#include <cstdint>
#include <cstdio>
#include <fstream>
#include <functional>
#include <memory>
#include <sstream>
#include <string>
#include <thread>
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

// Heap bytes in use (glibc: arena plus mmapped chunks) and resident bytes.
std::size_t heap_bytes() {
    const struct mallinfo2 mi = mallinfo2();
    return mi.uordblks + mi.hblkhd;
}
std::size_t rss_bytes() {
    long size = 0, resident = 0;
    if (FILE* f = std::fopen("/proc/self/statm", "r")) {
        if (std::fscanf(f, "%ld %ld", &size, &resident) != 2) resident = 0;
        std::fclose(f);
    }
    return static_cast<std::size_t>(resident) * static_cast<std::size_t>(sysconf(_SC_PAGESIZE));
}
struct Memory {
    long long heap, rss;
};
Memory memory_now() { return {static_cast<long long>(heap_bytes()), static_cast<long long>(rss_bytes())}; }

// Runs on the calling thread only: pins it to one CPU (single-thread timing) or frees it (before spawning threads,
// which inherit the mask).
void pin_to(int cpu) {
    cpu_set_t set;
    CPU_ZERO(&set);
    if (cpu >= 0) {
        CPU_SET(cpu, &set);
    } else {
        for (int c = 0; c < CPU_SETSIZE; ++c) CPU_SET(c, &set);
    }
    sched_setaffinity(0, sizeof set, &set);
}

std::string header(const char* kind, char** argv, bool governor, const std::string& method, const std::string& columns) {
    std::ostringstream h;
    h << "# fixture: " << kind << "\n"
      << "# coolprop: " << CoolProp::get_global_param_string("version") << " git="
      << CoolProp::get_global_param_string("gitrevision") << "\n"
      << "# cpu: " << argv[2] << "\n"
      << "# os: " << argv[3] << "\n";
    if (governor) h << "# governor: " << argv[4] << "\n";
    h << "# date: " << argv[5] << "\n"
      << "# compiler: " << __VERSION__ << " (CMAKE_BUILD_TYPE=Release, no -march)\n"
      << "# method: " << method << "\n"
      << "# columns: " << columns << "\n";
    return h.str();
}

}  // namespace

int main(int argc, char** argv) {
    if (argc != 6) {
        std::fprintf(stderr, "usage: coolprop_baseline <out.csv> <cpu model> <os> <governor> <date>\n");
        return 2;
    }
    const std::vector<std::string> fluids = {"Water", "Methane", "R134a", "n-Propane", "n-Heptane"};
    const int repeats = 7;
    const std::string out = argv[1], stem = out.substr(0, out.size() - 4);  // "<stem>.csv"

    // Memory first, while nothing has touched the fluid library yet.
    {
        std::ofstream mem(stem + ".memory.csv");
        mem << header("coolprop-baseline-memory/v1", argv, false,
                      "heap = glibc mallinfo2 (uordblks + hblkhd), rss = /proc/self/statm resident pages; per-state rows "
                      "are the mean over 100 states of that fluid held at once, after construction and after one QT "
                      "update at 0.7 Tc",
                      "measure,fluid,heap_bytes,rss_bytes");
        auto row = [&](const char* measure, const std::string& fluid, Memory m) {
            mem << measure << "," << fluid << "," << m.heap << "," << m.rss << "\n";
        };
        const Memory start = memory_now();
        std::shared_ptr<CoolProp::AbstractState> first(CoolProp::AbstractState::factory("HEOS", "Water"));
        const Memory loaded = memory_now();
        row("library_first_use", "Water", {loaded.heap - start.heap, loaded.rss - start.rss});
        const int n = 100;
        for (const auto& fluid : fluids) {
            std::vector<std::shared_ptr<CoolProp::AbstractState>> states;
            const Memory before = memory_now();
            for (int i = 0; i < n; ++i) states.emplace_back(CoolProp::AbstractState::factory("HEOS", fluid));
            const Memory built = memory_now();
            for (auto& s : states) s->update(CoolProp::QT_INPUTS, 0, 0.7 * s->T_critical());
            const Memory updated = memory_now();
            row("state", fluid, {(built.heap - before.heap) / n, (built.rss - before.rss) / n});
            row("state_after_qt", fluid, {(updated.heap - before.heap) / n, (updated.rss - before.rss) / n});
        }
        std::vector<std::shared_ptr<CoolProp::AbstractState>> all;
        std::stringstream names(CoolProp::get_global_param_string("FluidsList"));
        for (std::string name; std::getline(names, name, ',');) all.emplace_back(CoolProp::AbstractState::factory("HEOS", name));
        const Memory everything = memory_now();
        row("all_fluids_one_state_each", std::to_string(all.size()), {everything.heap - start.heap, everything.rss - start.rss});
    }

    pin_to(2);  // the single-thread timing rows run on one CPU
    std::ofstream csv(out);
    csv << header("coolprop-baseline/v1", argv, true,
                  "one thread; per row the median, min and max over " + std::to_string(repeats) +
                      " passes of the mean ns per state",
                  "workload,fluid,states,median_ns,min_ns,max_ns,grid");

    // The single-phase grid of each fluid, kept for the scaling rows.
    std::vector<std::vector<std::pair<double, double>>> single_grids;
    for (const auto& fluid : fluids) {
        std::shared_ptr<CoolProp::AbstractState> as(CoolProp::AbstractState::factory("HEOS", fluid));
        auto* heos = dynamic_cast<CoolProp::HelmholtzEOSMixtureBackend*>(as.get());
        const double tc = as->T_critical(), rhoc = as->rhomolar_critical(), tmax = as->Tmax(), ttriple = as->Ttriple();
        const Range t_single{"T", 1.05 * tc, std::min(1.5 * tc, tmax)}, rho_single{"rho", 0.1 * rhoc, 2.0 * rhoc};
        const Range t_sat{"T", ttriple + 0.05 * (tc - ttriple), 0.98 * tc}, q{"Q", 0.0, 1.0};
        const auto single = grid(10000, t_single, rho_single);
        const auto sat = grid(10000, t_sat, q);
        single_grids.push_back(single);
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

    // Thread scaling: DT + h + c_p, each thread on its own states (built here, before the clock), unpinned. A row is
    // one fluid on every thread, or "mixed": every thread takes the five fluids in turn. Speedup = throughput over
    // throughput at one thread of the same row.
    pin_to(-1);
    const std::vector<int> thread_counts = {1, 2, 4, 6, 12};
    const std::size_t per_thread = 2000;
    std::ofstream scaling(stem + ".scaling.csv");
    scaling << header("coolprop-baseline-scaling/v1", argv, true,
                      "DT + h + c_p on the (T, rho) grid; " + std::to_string(per_thread) +
                          " states per thread, one AbstractState per thread and fluid, threads unpinned; median wall "
                          "time over 5 passes",
                      "mode,threads,states_per_thread,median_ns_per_state,speedup");
    std::vector<std::string> modes = fluids;
    modes.push_back("mixed");
    for (std::size_t mode = 0; mode < modes.size(); ++mode) {
        const bool mixed = modes[mode] == "mixed";
        double single_thread_ns = 0;
        for (int threads : thread_counts) {
            std::vector<std::vector<std::shared_ptr<CoolProp::AbstractState>>> states(threads);
            for (auto& own : states) {
                for (std::size_t f = 0; f < fluids.size(); ++f) {
                    if (mixed || f == mode) own.emplace_back(CoolProp::AbstractState::factory("HEOS", fluids[f]));
                }
            }
            std::vector<double> walls;
            for (int pass = 0; pass < 5; ++pass) {
                std::atomic<int> ready{0};
                std::atomic<bool> go{false};
                std::vector<double> sums(threads, 0.0);
                std::vector<std::thread> pool;
                for (int t = 0; t < threads; ++t) {
                    pool.emplace_back([&, t] {
                        ready.fetch_add(1);
                        while (!go.load()) {
                        }
                        double acc = 0;
                        for (std::size_t i = 0; i < per_thread; ++i) {
                            const std::size_t k = mixed ? i % fluids.size() : 0;
                            const auto& point = single_grids[mixed ? k : mode][i];
                            auto& s = states[t][k];
                            s->update(CoolProp::DmolarT_INPUTS, point.second, point.first);
                            acc += s->hmolar() + s->cpmolar();
                        }
                        sums[t] = acc;
                    });
                }
                while (ready.load() < threads) {
                }
                const auto t0 = std::chrono::steady_clock::now();
                go.store(true);
                for (auto& th : pool) th.join();
                walls.push_back(std::chrono::duration<double, std::nano>(std::chrono::steady_clock::now() - t0).count());
                for (double s : sums) sink = sink + s;
            }
            std::sort(walls.begin(), walls.end());
            const double ns = walls[walls.size() / 2] / static_cast<double>(per_thread);
            if (threads == 1) single_thread_ns = ns;
            const double speedup = threads == 1 ? 1.0 : threads * single_thread_ns / ns;
            char buf[160];
            std::snprintf(buf, sizeof buf, "%s,%d,%zu,%.4g,%.4g\n", modes[mode].c_str(), threads, per_thread, ns, speedup);
            scaling << buf;
            std::fprintf(stderr, "scaling %-10s %2d threads: %8.1f ns/state, speedup %.2f\n", modes[mode].c_str(), threads, ns, speedup);
        }
    }
    std::fprintf(stderr, "checksum %.17g\n", static_cast<double>(sink));
    return 0;
}
