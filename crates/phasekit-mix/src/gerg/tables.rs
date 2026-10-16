// GERG-2008's coefficients (Kunz and Wagner 2012), transcribed from NIST's AGA8 code (usnistgov/AGA8 at
// 3bdb9ab8, AGA8CODE/C/GERG2008.cpp, SetupGERG; public domain, user decision MX7) as it assigns them, before its
// setup rescales them. `cargo xtask`'s `gerg_2008_is_aga8s` checks every number against the fetched copy; each
// component's CAS number is phasekit's key into the registry, not AGA8's.

use crate::data::{DepartureData, Kind, Term};
use crate::gerg::{Binary, Component};

const SOURCE: &str = "Kunz-JCED-2012";

const ZERO: Term = Term { n: 0.0, t: 0.0, d: 0.0, l: 0.0, eta: 0.0, epsilon: 0.0, beta: 0.0, gamma: 0.0 };

/// Methane: 6 polynomial and 18 exponential terms.
static METHANE: [Term; 24] = [
    Term { n: 0.57335704239162, d: 1.0, t: 0.125, l: 0.0, ..ZERO },
    Term { n: -1.676068752373, d: 1.0, t: 1.125, l: 0.0, ..ZERO },
    Term { n: 0.23405291834916, d: 2.0, t: 0.375, l: 0.0, ..ZERO },
    Term { n: -0.21947376343441, d: 2.0, t: 1.125, l: 0.0, ..ZERO },
    Term { n: 0.016369201404128, d: 4.0, t: 0.625, l: 0.0, ..ZERO },
    Term { n: 0.01500440638928, d: 4.0, t: 1.5, l: 0.0, ..ZERO },
    Term { n: 0.098990489492918, d: 1.0, t: 0.625, l: 1.0, ..ZERO },
    Term { n: 0.58382770929055, d: 1.0, t: 2.625, l: 1.0, ..ZERO },
    Term { n: -0.7478686756039, d: 1.0, t: 2.75, l: 1.0, ..ZERO },
    Term { n: 0.30033302857974, d: 2.0, t: 2.125, l: 1.0, ..ZERO },
    Term { n: 0.20985543806568, d: 3.0, t: 2.0, l: 1.0, ..ZERO },
    Term { n: -0.018590151133061, d: 6.0, t: 1.75, l: 1.0, ..ZERO },
    Term { n: -0.15782558339049, d: 2.0, t: 4.5, l: 2.0, ..ZERO },
    Term { n: 0.12716735220791, d: 3.0, t: 4.75, l: 2.0, ..ZERO },
    Term { n: -0.032019743894346, d: 3.0, t: 5.0, l: 2.0, ..ZERO },
    Term { n: -0.068049729364536, d: 4.0, t: 4.0, l: 2.0, ..ZERO },
    Term { n: 0.024291412853736, d: 4.0, t: 4.5, l: 2.0, ..ZERO },
    Term { n: 0.0051440451639444, d: 2.0, t: 7.5, l: 3.0, ..ZERO },
    Term { n: -0.019084949733532, d: 3.0, t: 14.0, l: 3.0, ..ZERO },
    Term { n: 0.0055229677241291, d: 4.0, t: 11.5, l: 3.0, ..ZERO },
    Term { n: -0.0044197392976085, d: 5.0, t: 26.0, l: 6.0, ..ZERO },
    Term { n: 0.040061416708429, d: 6.0, t: 28.0, l: 6.0, ..ZERO },
    Term { n: -0.033752085907575, d: 6.0, t: 30.0, l: 6.0, ..ZERO },
    Term { n: -0.0025127658213357, d: 7.0, t: 16.0, l: 6.0, ..ZERO },
];

/// Nitrogen: 6 polynomial and 18 exponential terms.
static NITROGEN: [Term; 24] = [
    Term { n: 0.59889711801201, d: 1.0, t: 0.125, l: 0.0, ..ZERO },
    Term { n: -1.6941557480731, d: 1.0, t: 1.125, l: 0.0, ..ZERO },
    Term { n: 0.24579736191718, d: 2.0, t: 0.375, l: 0.0, ..ZERO },
    Term { n: -0.23722456755175, d: 2.0, t: 1.125, l: 0.0, ..ZERO },
    Term { n: 0.017954918715141, d: 4.0, t: 0.625, l: 0.0, ..ZERO },
    Term { n: 0.014592875720215, d: 4.0, t: 1.5, l: 0.0, ..ZERO },
    Term { n: 0.10008065936206, d: 1.0, t: 0.625, l: 1.0, ..ZERO },
    Term { n: 0.73157115385532, d: 1.0, t: 2.625, l: 1.0, ..ZERO },
    Term { n: -0.88372272336366, d: 1.0, t: 2.75, l: 1.0, ..ZERO },
    Term { n: 0.31887660246708, d: 2.0, t: 2.125, l: 1.0, ..ZERO },
    Term { n: 0.20766491728799, d: 3.0, t: 2.0, l: 1.0, ..ZERO },
    Term { n: -0.019379315454158, d: 6.0, t: 1.75, l: 1.0, ..ZERO },
    Term { n: -0.16936641554983, d: 2.0, t: 4.5, l: 2.0, ..ZERO },
    Term { n: 0.13546846041701, d: 3.0, t: 4.75, l: 2.0, ..ZERO },
    Term { n: -0.033066712095307, d: 3.0, t: 5.0, l: 2.0, ..ZERO },
    Term { n: -0.060690817018557, d: 4.0, t: 4.0, l: 2.0, ..ZERO },
    Term { n: 0.012797548292871, d: 4.0, t: 4.5, l: 2.0, ..ZERO },
    Term { n: 0.0058743664107299, d: 2.0, t: 7.5, l: 3.0, ..ZERO },
    Term { n: -0.018451951971969, d: 3.0, t: 14.0, l: 3.0, ..ZERO },
    Term { n: 0.0047226622042472, d: 4.0, t: 11.5, l: 3.0, ..ZERO },
    Term { n: -0.0052024079680599, d: 5.0, t: 26.0, l: 6.0, ..ZERO },
    Term { n: 0.043563505956635, d: 6.0, t: 28.0, l: 6.0, ..ZERO },
    Term { n: -0.036251690750939, d: 6.0, t: 30.0, l: 6.0, ..ZERO },
    Term { n: -0.0028974026866543, d: 7.0, t: 16.0, l: 6.0, ..ZERO },
];

/// Carbon dioxide: 4 polynomial and 18 exponential terms.
static CARBON_DIOXIDE: [Term; 22] = [
    Term { n: 0.52646564804653, d: 1.0, t: 0.0, l: 0.0, ..ZERO },
    Term { n: -1.4995725042592, d: 1.0, t: 1.25, l: 0.0, ..ZERO },
    Term { n: 0.27329786733782, d: 2.0, t: 1.625, l: 0.0, ..ZERO },
    Term { n: 0.12949500022786, d: 3.0, t: 0.375, l: 0.0, ..ZERO },
    Term { n: 0.15404088341841, d: 3.0, t: 0.375, l: 1.0, ..ZERO },
    Term { n: -0.58186950946814, d: 3.0, t: 1.375, l: 1.0, ..ZERO },
    Term { n: -0.18022494838296, d: 4.0, t: 1.125, l: 1.0, ..ZERO },
    Term { n: -0.095389904072812, d: 5.0, t: 1.375, l: 1.0, ..ZERO },
    Term { n: -0.0080486819317679, d: 6.0, t: 0.125, l: 1.0, ..ZERO },
    Term { n: -0.03554775127309, d: 6.0, t: 1.625, l: 1.0, ..ZERO },
    Term { n: -0.28079014882405, d: 1.0, t: 3.75, l: 2.0, ..ZERO },
    Term { n: -0.082435890081677, d: 4.0, t: 3.5, l: 2.0, ..ZERO },
    Term { n: 0.010832427979006, d: 1.0, t: 7.5, l: 3.0, ..ZERO },
    Term { n: -0.0067073993161097, d: 1.0, t: 8.0, l: 3.0, ..ZERO },
    Term { n: -0.0046827907600524, d: 3.0, t: 6.0, l: 3.0, ..ZERO },
    Term { n: -0.028359911832177, d: 3.0, t: 16.0, l: 3.0, ..ZERO },
    Term { n: 0.019500174744098, d: 4.0, t: 11.0, l: 3.0, ..ZERO },
    Term { n: -0.21609137507166, d: 5.0, t: 24.0, l: 5.0, ..ZERO },
    Term { n: 0.43772794926972, d: 5.0, t: 26.0, l: 5.0, ..ZERO },
    Term { n: -0.22130790113593, d: 5.0, t: 28.0, l: 5.0, ..ZERO },
    Term { n: 0.015190189957331, d: 5.0, t: 24.0, l: 6.0, ..ZERO },
    Term { n: -0.0153809489533, d: 5.0, t: 26.0, l: 6.0, ..ZERO },
];

/// Ethane: 6 polynomial and 18 exponential terms.
static ETHANE: [Term; 24] = [
    Term { n: 0.63596780450714, d: 1.0, t: 0.125, l: 0.0, ..ZERO },
    Term { n: -1.7377981785459, d: 1.0, t: 1.125, l: 0.0, ..ZERO },
    Term { n: 0.28914060926272, d: 2.0, t: 0.375, l: 0.0, ..ZERO },
    Term { n: -0.33714276845694, d: 2.0, t: 1.125, l: 0.0, ..ZERO },
    Term { n: 0.022405964699561, d: 4.0, t: 0.625, l: 0.0, ..ZERO },
    Term { n: 0.015715424886913, d: 4.0, t: 1.5, l: 0.0, ..ZERO },
    Term { n: 0.11450634253745, d: 1.0, t: 0.625, l: 1.0, ..ZERO },
    Term { n: 1.0612049379745, d: 1.0, t: 2.625, l: 1.0, ..ZERO },
    Term { n: -1.2855224439423, d: 1.0, t: 2.75, l: 1.0, ..ZERO },
    Term { n: 0.39414630777652, d: 2.0, t: 2.125, l: 1.0, ..ZERO },
    Term { n: 0.31390924682041, d: 3.0, t: 2.0, l: 1.0, ..ZERO },
    Term { n: -0.021592277117247, d: 6.0, t: 1.75, l: 1.0, ..ZERO },
    Term { n: -0.21723666564905, d: 2.0, t: 4.5, l: 2.0, ..ZERO },
    Term { n: -0.28999574439489, d: 3.0, t: 4.75, l: 2.0, ..ZERO },
    Term { n: 0.42321173025732, d: 3.0, t: 5.0, l: 2.0, ..ZERO },
    Term { n: 0.04643410025926, d: 4.0, t: 4.0, l: 2.0, ..ZERO },
    Term { n: -0.13138398329741, d: 4.0, t: 4.5, l: 2.0, ..ZERO },
    Term { n: 0.011492850364368, d: 2.0, t: 7.5, l: 3.0, ..ZERO },
    Term { n: -0.033387688429909, d: 3.0, t: 14.0, l: 3.0, ..ZERO },
    Term { n: 0.015183171583644, d: 4.0, t: 11.5, l: 3.0, ..ZERO },
    Term { n: -0.0047610805647657, d: 5.0, t: 26.0, l: 6.0, ..ZERO },
    Term { n: 0.046917166277885, d: 6.0, t: 28.0, l: 6.0, ..ZERO },
    Term { n: -0.039401755804649, d: 6.0, t: 30.0, l: 6.0, ..ZERO },
    Term { n: -0.0032569956247611, d: 7.0, t: 16.0, l: 6.0, ..ZERO },
];

/// Propane: 6 polynomial and 6 exponential terms.
static PROPANE: [Term; 12] = [
    Term { n: 1.0403973107358, d: 1.0, t: 0.25, l: 0.0, ..ZERO },
    Term { n: -2.8318404081403, d: 1.0, t: 1.125, l: 0.0, ..ZERO },
    Term { n: 0.84393809606294, d: 1.0, t: 1.5, l: 0.0, ..ZERO },
    Term { n: -0.076559591850023, d: 2.0, t: 1.375, l: 0.0, ..ZERO },
    Term { n: 0.09469737305728, d: 3.0, t: 0.25, l: 0.0, ..ZERO },
    Term { n: 0.00024796475497006, d: 7.0, t: 0.875, l: 0.0, ..ZERO },
    Term { n: 0.2774376042287, d: 2.0, t: 0.625, l: 1.0, ..ZERO },
    Term { n: -0.043846000648377, d: 5.0, t: 1.75, l: 1.0, ..ZERO },
    Term { n: -0.2699106478435, d: 1.0, t: 3.625, l: 2.0, ..ZERO },
    Term { n: -0.06931341308986, d: 4.0, t: 3.625, l: 2.0, ..ZERO },
    Term { n: -0.029632145981653, d: 3.0, t: 14.5, l: 3.0, ..ZERO },
    Term { n: 0.01404012675138, d: 4.0, t: 12.0, l: 3.0, ..ZERO },
];

/// Isobutane: 6 polynomial and 6 exponential terms.
static ISOBUTANE: [Term; 12] = [
    Term { n: 1.04293315891, d: 1.0, t: 0.25, l: 0.0, ..ZERO },
    Term { n: -2.8184272548892, d: 1.0, t: 1.125, l: 0.0, ..ZERO },
    Term { n: 0.8617623239785, d: 1.0, t: 1.5, l: 0.0, ..ZERO },
    Term { n: -0.10613619452487, d: 2.0, t: 1.375, l: 0.0, ..ZERO },
    Term { n: 0.098615749302134, d: 3.0, t: 0.25, l: 0.0, ..ZERO },
    Term { n: 0.00023948208682322, d: 7.0, t: 0.875, l: 0.0, ..ZERO },
    Term { n: 0.3033000485695, d: 2.0, t: 0.625, l: 1.0, ..ZERO },
    Term { n: -0.041598156135099, d: 5.0, t: 1.75, l: 1.0, ..ZERO },
    Term { n: -0.29991937470058, d: 1.0, t: 3.625, l: 2.0, ..ZERO },
    Term { n: -0.080369342764109, d: 4.0, t: 3.625, l: 2.0, ..ZERO },
    Term { n: -0.029761373251151, d: 3.0, t: 14.5, l: 3.0, ..ZERO },
    Term { n: 0.01305963030314, d: 4.0, t: 12.0, l: 3.0, ..ZERO },
];

/// n-Butane: 6 polynomial and 6 exponential terms.
static N_BUTANE: [Term; 12] = [
    Term { n: 1.0626277411455, d: 1.0, t: 0.25, l: 0.0, ..ZERO },
    Term { n: -2.862095182835, d: 1.0, t: 1.125, l: 0.0, ..ZERO },
    Term { n: 0.88738233403777, d: 1.0, t: 1.5, l: 0.0, ..ZERO },
    Term { n: -0.12570581155345, d: 2.0, t: 1.375, l: 0.0, ..ZERO },
    Term { n: 0.10286308708106, d: 3.0, t: 0.25, l: 0.0, ..ZERO },
    Term { n: 0.00025358040602654, d: 7.0, t: 0.875, l: 0.0, ..ZERO },
    Term { n: 0.32325200233982, d: 2.0, t: 0.625, l: 1.0, ..ZERO },
    Term { n: -0.037950761057432, d: 5.0, t: 1.75, l: 1.0, ..ZERO },
    Term { n: -0.32534802014452, d: 1.0, t: 3.625, l: 2.0, ..ZERO },
    Term { n: -0.079050969051011, d: 4.0, t: 3.625, l: 2.0, ..ZERO },
    Term { n: -0.020636720547775, d: 3.0, t: 14.5, l: 3.0, ..ZERO },
    Term { n: 0.005705380933475, d: 4.0, t: 12.0, l: 3.0, ..ZERO },
];

/// Isopentane: 6 polynomial and 6 exponential terms.
static ISOPENTANE: [Term; 12] = [
    Term { n: 1.0963, d: 1.0, t: 0.25, l: 0.0, ..ZERO },
    Term { n: -3.0402, d: 1.0, t: 1.125, l: 0.0, ..ZERO },
    Term { n: 1.0317, d: 1.0, t: 1.5, l: 0.0, ..ZERO },
    Term { n: -0.1541, d: 2.0, t: 1.375, l: 0.0, ..ZERO },
    Term { n: 0.11535, d: 3.0, t: 0.25, l: 0.0, ..ZERO },
    Term { n: 0.00029809, d: 7.0, t: 0.875, l: 0.0, ..ZERO },
    Term { n: 0.39571, d: 2.0, t: 0.625, l: 1.0, ..ZERO },
    Term { n: -0.045881, d: 5.0, t: 1.75, l: 1.0, ..ZERO },
    Term { n: -0.35804, d: 1.0, t: 3.625, l: 2.0, ..ZERO },
    Term { n: -0.10107, d: 4.0, t: 3.625, l: 2.0, ..ZERO },
    Term { n: -0.035484, d: 3.0, t: 14.5, l: 3.0, ..ZERO },
    Term { n: 0.018156, d: 4.0, t: 12.0, l: 3.0, ..ZERO },
];

/// n-Pentane: 6 polynomial and 6 exponential terms.
static N_PENTANE: [Term; 12] = [
    Term { n: 1.0968643098001, d: 1.0, t: 0.25, l: 0.0, ..ZERO },
    Term { n: -2.9988888298061, d: 1.0, t: 1.125, l: 0.0, ..ZERO },
    Term { n: 0.99516886799212, d: 1.0, t: 1.5, l: 0.0, ..ZERO },
    Term { n: -0.16170708558539, d: 2.0, t: 1.375, l: 0.0, ..ZERO },
    Term { n: 0.11334460072775, d: 3.0, t: 0.25, l: 0.0, ..ZERO },
    Term { n: 0.00026760595150748, d: 7.0, t: 0.875, l: 0.0, ..ZERO },
    Term { n: 0.40979881986931, d: 2.0, t: 0.625, l: 1.0, ..ZERO },
    Term { n: -0.040876423083075, d: 5.0, t: 1.75, l: 1.0, ..ZERO },
    Term { n: -0.38169482469447, d: 1.0, t: 3.625, l: 2.0, ..ZERO },
    Term { n: -0.10931956843993, d: 4.0, t: 3.625, l: 2.0, ..ZERO },
    Term { n: -0.03207322332799, d: 3.0, t: 14.5, l: 3.0, ..ZERO },
    Term { n: 0.016877016216975, d: 4.0, t: 12.0, l: 3.0, ..ZERO },
];

/// n-Hexane: 6 polynomial and 6 exponential terms.
static N_HEXANE: [Term; 12] = [
    Term { n: 1.0553238013661, d: 1.0, t: 0.25, l: 0.0, ..ZERO },
    Term { n: -2.6120615890629, d: 1.0, t: 1.125, l: 0.0, ..ZERO },
    Term { n: 0.7661388296726, d: 1.0, t: 1.5, l: 0.0, ..ZERO },
    Term { n: -0.29770320622459, d: 2.0, t: 1.375, l: 0.0, ..ZERO },
    Term { n: 0.11879907733358, d: 3.0, t: 0.25, l: 0.0, ..ZERO },
    Term { n: 0.00027922861062617, d: 7.0, t: 0.875, l: 0.0, ..ZERO },
    Term { n: 0.46347589844105, d: 2.0, t: 0.625, l: 1.0, ..ZERO },
    Term { n: 0.011433196980297, d: 5.0, t: 1.75, l: 1.0, ..ZERO },
    Term { n: -0.48256968738131, d: 1.0, t: 3.625, l: 2.0, ..ZERO },
    Term { n: -0.093750558924659, d: 4.0, t: 3.625, l: 2.0, ..ZERO },
    Term { n: -0.0067273247155994, d: 3.0, t: 14.5, l: 3.0, ..ZERO },
    Term { n: -0.0051141583585428, d: 4.0, t: 12.0, l: 3.0, ..ZERO },
];

/// n-Heptane: 6 polynomial and 6 exponential terms.
static N_HEPTANE: [Term; 12] = [
    Term { n: 1.0543747645262, d: 1.0, t: 0.25, l: 0.0, ..ZERO },
    Term { n: -2.6500681506144, d: 1.0, t: 1.125, l: 0.0, ..ZERO },
    Term { n: 0.81730047827543, d: 1.0, t: 1.5, l: 0.0, ..ZERO },
    Term { n: -0.30451391253428, d: 2.0, t: 1.375, l: 0.0, ..ZERO },
    Term { n: 0.122538687108, d: 3.0, t: 0.25, l: 0.0, ..ZERO },
    Term { n: 0.00027266472743928, d: 7.0, t: 0.875, l: 0.0, ..ZERO },
    Term { n: 0.4986582568167, d: 2.0, t: 0.625, l: 1.0, ..ZERO },
    Term { n: -0.00071432815084176, d: 5.0, t: 1.75, l: 1.0, ..ZERO },
    Term { n: -0.5423689552545, d: 1.0, t: 3.625, l: 2.0, ..ZERO },
    Term { n: -0.13801821610756, d: 4.0, t: 3.625, l: 2.0, ..ZERO },
    Term { n: -0.0061595287380011, d: 3.0, t: 14.5, l: 3.0, ..ZERO },
    Term { n: 0.00048602510393022, d: 4.0, t: 12.0, l: 3.0, ..ZERO },
];

/// n-Octane: 6 polynomial and 6 exponential terms.
static N_OCTANE: [Term; 12] = [
    Term { n: 1.0722544875633, d: 1.0, t: 0.25, l: 0.0, ..ZERO },
    Term { n: -2.4632951172003, d: 1.0, t: 1.125, l: 0.0, ..ZERO },
    Term { n: 0.65386674054928, d: 1.0, t: 1.5, l: 0.0, ..ZERO },
    Term { n: -0.36324974085628, d: 2.0, t: 1.375, l: 0.0, ..ZERO },
    Term { n: 0.12713269626764, d: 3.0, t: 0.25, l: 0.0, ..ZERO },
    Term { n: 0.0003071357277793, d: 7.0, t: 0.875, l: 0.0, ..ZERO },
    Term { n: 0.5265685698754, d: 2.0, t: 0.625, l: 1.0, ..ZERO },
    Term { n: 0.019362862857653, d: 5.0, t: 1.75, l: 1.0, ..ZERO },
    Term { n: -0.58939426849155, d: 1.0, t: 3.625, l: 2.0, ..ZERO },
    Term { n: -0.14069963991934, d: 4.0, t: 3.625, l: 2.0, ..ZERO },
    Term { n: -0.0078966330500036, d: 3.0, t: 14.5, l: 3.0, ..ZERO },
    Term { n: 0.0033036597968109, d: 4.0, t: 12.0, l: 3.0, ..ZERO },
];

/// n-Nonane: 6 polynomial and 6 exponential terms.
static N_NONANE: [Term; 12] = [
    Term { n: 1.1151, d: 1.0, t: 0.25, l: 0.0, ..ZERO },
    Term { n: -2.702, d: 1.0, t: 1.125, l: 0.0, ..ZERO },
    Term { n: 0.83416, d: 1.0, t: 1.5, l: 0.0, ..ZERO },
    Term { n: -0.38828, d: 2.0, t: 1.375, l: 0.0, ..ZERO },
    Term { n: 0.1376, d: 3.0, t: 0.25, l: 0.0, ..ZERO },
    Term { n: 0.00028185, d: 7.0, t: 0.875, l: 0.0, ..ZERO },
    Term { n: 0.62037, d: 2.0, t: 0.625, l: 1.0, ..ZERO },
    Term { n: 0.015847, d: 5.0, t: 1.75, l: 1.0, ..ZERO },
    Term { n: -0.61726, d: 1.0, t: 3.625, l: 2.0, ..ZERO },
    Term { n: -0.15043, d: 4.0, t: 3.625, l: 2.0, ..ZERO },
    Term { n: -0.012982, d: 3.0, t: 14.5, l: 3.0, ..ZERO },
    Term { n: 0.0044325, d: 4.0, t: 12.0, l: 3.0, ..ZERO },
];

/// n-Decane: 6 polynomial and 6 exponential terms.
static N_DECANE: [Term; 12] = [
    Term { n: 1.0461, d: 1.0, t: 0.25, l: 0.0, ..ZERO },
    Term { n: -2.4807, d: 1.0, t: 1.125, l: 0.0, ..ZERO },
    Term { n: 0.74372, d: 1.0, t: 1.5, l: 0.0, ..ZERO },
    Term { n: -0.52579, d: 2.0, t: 1.375, l: 0.0, ..ZERO },
    Term { n: 0.15315, d: 3.0, t: 0.25, l: 0.0, ..ZERO },
    Term { n: 0.00032865, d: 7.0, t: 0.875, l: 0.0, ..ZERO },
    Term { n: 0.84178, d: 2.0, t: 0.625, l: 1.0, ..ZERO },
    Term { n: 0.055424, d: 5.0, t: 1.75, l: 1.0, ..ZERO },
    Term { n: -0.73555, d: 1.0, t: 3.625, l: 2.0, ..ZERO },
    Term { n: -0.18507, d: 4.0, t: 3.625, l: 2.0, ..ZERO },
    Term { n: -0.020775, d: 3.0, t: 14.5, l: 3.0, ..ZERO },
    Term { n: 0.012335, d: 4.0, t: 12.0, l: 3.0, ..ZERO },
];

/// Hydrogen: 5 polynomial and 9 exponential terms.
static HYDROGEN: [Term; 14] = [
    Term { n: 5.3579928451252, d: 1.0, t: 0.5, l: 0.0, ..ZERO },
    Term { n: -6.2050252530595, d: 1.0, t: 0.625, l: 0.0, ..ZERO },
    Term { n: 0.13830241327086, d: 2.0, t: 0.375, l: 0.0, ..ZERO },
    Term { n: -0.071397954896129, d: 2.0, t: 0.625, l: 0.0, ..ZERO },
    Term { n: 0.015474053959733, d: 4.0, t: 1.125, l: 0.0, ..ZERO },
    Term { n: -0.14976806405771, d: 1.0, t: 2.625, l: 1.0, ..ZERO },
    Term { n: -0.026368723988451, d: 5.0, t: 0.0, l: 1.0, ..ZERO },
    Term { n: 0.056681303156066, d: 5.0, t: 0.25, l: 1.0, ..ZERO },
    Term { n: -0.060063958030436, d: 5.0, t: 1.375, l: 1.0, ..ZERO },
    Term { n: -0.45043942027132, d: 1.0, t: 4.0, l: 2.0, ..ZERO },
    Term { n: 0.424788402445, d: 1.0, t: 4.25, l: 2.0, ..ZERO },
    Term { n: -0.021997640827139, d: 2.0, t: 5.0, l: 3.0, ..ZERO },
    Term { n: -0.01049952137453, d: 5.0, t: 8.0, l: 3.0, ..ZERO },
    Term { n: -0.0028955902866816, d: 1.0, t: 8.0, l: 5.0, ..ZERO },
];

/// Oxygen: 6 polynomial and 6 exponential terms.
static OXYGEN: [Term; 12] = [
    Term { n: 0.88878286369701, d: 1.0, t: 0.25, l: 0.0, ..ZERO },
    Term { n: -2.4879433312148, d: 1.0, t: 1.125, l: 0.0, ..ZERO },
    Term { n: 0.59750190775886, d: 1.0, t: 1.5, l: 0.0, ..ZERO },
    Term { n: 0.0096501817061881, d: 2.0, t: 1.375, l: 0.0, ..ZERO },
    Term { n: 0.07197042871277, d: 3.0, t: 0.25, l: 0.0, ..ZERO },
    Term { n: 0.00022337443000195, d: 7.0, t: 0.875, l: 0.0, ..ZERO },
    Term { n: 0.18558686391474, d: 2.0, t: 0.625, l: 1.0, ..ZERO },
    Term { n: -0.03812936803576, d: 5.0, t: 1.75, l: 1.0, ..ZERO },
    Term { n: -0.15352245383006, d: 1.0, t: 3.625, l: 2.0, ..ZERO },
    Term { n: -0.026726814910919, d: 4.0, t: 3.625, l: 2.0, ..ZERO },
    Term { n: -0.025675298677127, d: 3.0, t: 14.5, l: 3.0, ..ZERO },
    Term { n: 0.0095714302123668, d: 4.0, t: 12.0, l: 3.0, ..ZERO },
];

/// Carbon monoxide: 6 polynomial and 6 exponential terms.
static CARBON_MONOXIDE: [Term; 12] = [
    Term { n: 0.90554, d: 1.0, t: 0.25, l: 0.0, ..ZERO },
    Term { n: -2.4515, d: 1.0, t: 1.125, l: 0.0, ..ZERO },
    Term { n: 0.53149, d: 1.0, t: 1.5, l: 0.0, ..ZERO },
    Term { n: 0.024173, d: 2.0, t: 1.375, l: 0.0, ..ZERO },
    Term { n: 0.072156, d: 3.0, t: 0.25, l: 0.0, ..ZERO },
    Term { n: 0.00018818, d: 7.0, t: 0.875, l: 0.0, ..ZERO },
    Term { n: 0.19405, d: 2.0, t: 0.625, l: 1.0, ..ZERO },
    Term { n: -0.043268, d: 5.0, t: 1.75, l: 1.0, ..ZERO },
    Term { n: -0.12778, d: 1.0, t: 3.625, l: 2.0, ..ZERO },
    Term { n: -0.027896, d: 4.0, t: 3.625, l: 2.0, ..ZERO },
    Term { n: -0.034154, d: 3.0, t: 14.5, l: 3.0, ..ZERO },
    Term { n: 0.016329, d: 4.0, t: 12.0, l: 3.0, ..ZERO },
];

/// Water: 7 polynomial and 9 exponential terms.
static WATER: [Term; 16] = [
    Term { n: 0.82728408749586, d: 1.0, t: 0.5, l: 0.0, ..ZERO },
    Term { n: -1.8602220416584, d: 1.0, t: 1.25, l: 0.0, ..ZERO },
    Term { n: -1.1199009613744, d: 1.0, t: 1.875, l: 0.0, ..ZERO },
    Term { n: 0.15635753976056, d: 2.0, t: 0.125, l: 0.0, ..ZERO },
    Term { n: 0.87375844859025, d: 2.0, t: 1.5, l: 0.0, ..ZERO },
    Term { n: -0.36674403715731, d: 3.0, t: 1.0, l: 0.0, ..ZERO },
    Term { n: 0.053987893432436, d: 4.0, t: 0.75, l: 0.0, ..ZERO },
    Term { n: 1.0957690214499, d: 1.0, t: 1.5, l: 1.0, ..ZERO },
    Term { n: 0.053213037828563, d: 5.0, t: 0.625, l: 1.0, ..ZERO },
    Term { n: 0.013050533930825, d: 5.0, t: 2.625, l: 1.0, ..ZERO },
    Term { n: -0.41079520434476, d: 1.0, t: 5.0, l: 2.0, ..ZERO },
    Term { n: 0.1463744334412, d: 2.0, t: 4.0, l: 2.0, ..ZERO },
    Term { n: -0.055726838623719, d: 4.0, t: 4.5, l: 2.0, ..ZERO },
    Term { n: -0.0112017741438, d: 4.0, t: 3.0, l: 3.0, ..ZERO },
    Term { n: -0.0066062758068099, d: 1.0, t: 4.0, l: 5.0, ..ZERO },
    Term { n: 0.0046918522004538, d: 1.0, t: 6.0, l: 5.0, ..ZERO },
];

/// Hydrogen sulfide: 6 polynomial and 6 exponential terms.
static HYDROGEN_SULFIDE: [Term; 12] = [
    Term { n: 0.87641, d: 1.0, t: 0.25, l: 0.0, ..ZERO },
    Term { n: -2.0367, d: 1.0, t: 1.125, l: 0.0, ..ZERO },
    Term { n: 0.21634, d: 1.0, t: 1.5, l: 0.0, ..ZERO },
    Term { n: -0.050199, d: 2.0, t: 1.375, l: 0.0, ..ZERO },
    Term { n: 0.066994, d: 3.0, t: 0.25, l: 0.0, ..ZERO },
    Term { n: 0.00019076, d: 7.0, t: 0.875, l: 0.0, ..ZERO },
    Term { n: 0.20227, d: 2.0, t: 0.625, l: 1.0, ..ZERO },
    Term { n: -0.0045348, d: 5.0, t: 1.75, l: 1.0, ..ZERO },
    Term { n: -0.2223, d: 1.0, t: 3.625, l: 2.0, ..ZERO },
    Term { n: -0.034714, d: 4.0, t: 3.625, l: 2.0, ..ZERO },
    Term { n: -0.014885, d: 3.0, t: 14.5, l: 3.0, ..ZERO },
    Term { n: 0.0074154, d: 4.0, t: 12.0, l: 3.0, ..ZERO },
];

/// Helium: 4 polynomial and 8 exponential terms.
static HELIUM: [Term; 12] = [
    Term { n: -0.45579024006737, d: 1.0, t: 0.0, l: 0.0, ..ZERO },
    Term { n: 1.2516390754925, d: 1.0, t: 0.125, l: 0.0, ..ZERO },
    Term { n: -1.5438231650621, d: 1.0, t: 0.75, l: 0.0, ..ZERO },
    Term { n: 0.020467489707221, d: 4.0, t: 1.0, l: 0.0, ..ZERO },
    Term { n: -0.34476212380781, d: 1.0, t: 0.75, l: 1.0, ..ZERO },
    Term { n: -0.020858459512787, d: 3.0, t: 2.625, l: 1.0, ..ZERO },
    Term { n: 0.016227414711778, d: 5.0, t: 0.125, l: 1.0, ..ZERO },
    Term { n: -0.057471818200892, d: 5.0, t: 1.25, l: 1.0, ..ZERO },
    Term { n: 0.019462416430715, d: 5.0, t: 2.0, l: 1.0, ..ZERO },
    Term { n: -0.03329568012302, d: 2.0, t: 1.0, l: 2.0, ..ZERO },
    Term { n: -0.010863577372367, d: 1.0, t: 4.5, l: 3.0, ..ZERO },
    Term { n: -0.022173365245954, d: 2.0, t: 5.0, l: 3.0, ..ZERO },
];

/// Argon: 6 polynomial and 6 exponential terms.
static ARGON: [Term; 12] = [
    Term { n: 0.85095714803969, d: 1.0, t: 0.25, l: 0.0, ..ZERO },
    Term { n: -2.400322294348, d: 1.0, t: 1.125, l: 0.0, ..ZERO },
    Term { n: 0.54127841476466, d: 1.0, t: 1.5, l: 0.0, ..ZERO },
    Term { n: 0.016919770692538, d: 2.0, t: 1.375, l: 0.0, ..ZERO },
    Term { n: 0.068825965019035, d: 3.0, t: 0.25, l: 0.0, ..ZERO },
    Term { n: 0.00021428032815338, d: 7.0, t: 0.875, l: 0.0, ..ZERO },
    Term { n: 0.17429895321992, d: 2.0, t: 0.625, l: 1.0, ..ZERO },
    Term { n: -0.033654495604194, d: 5.0, t: 1.75, l: 1.0, ..ZERO },
    Term { n: -0.13526799857691, d: 1.0, t: 3.625, l: 2.0, ..ZERO },
    Term { n: -0.016387350791552, d: 4.0, t: 3.625, l: 2.0, ..ZERO },
    Term { n: -0.024987666851475, d: 3.0, t: 14.5, l: 3.0, ..ZERO },
    Term { n: 0.0088769204815709, d: 4.0, t: 12.0, l: 3.0, ..ZERO },
];

/// The 21 components in AGA8's order (index + 1 is AGA8's).
pub(crate) static COMPONENTS: [Component; 21] = [
    Component { name: "Methane", cas: "74-82-8", molar_mass: 16.04246, rho_c: 10.139342719, t_c: 190.564, residual: &METHANE, n0: [29.83843397, -15999.69151, 4.00088, 0.76315, 0.0046, 8.74432, -4.46921], theta: [820.659, 178.41, 1062.82, 1090.53] },
    Component { name: "Nitrogen", cas: "7727-37-9", molar_mass: 28.0134, rho_c: 11.1839, t_c: 126.192, residual: &NITROGEN, n0: [17.56770785, -2801.729072, 3.50031, 0.13732, -0.1466, 0.90066, 0.0], theta: [662.738, 680.562, 1740.06, 0.0] },
    Component { name: "Carbon dioxide", cas: "124-38-9", molar_mass: 44.0095, rho_c: 10.624978698, t_c: 304.1282, residual: &CARBON_DIOXIDE, n0: [20.65844696, -4902.171516, 3.50002, 2.04452, -1.06044, 2.03366, 0.01393], theta: [919.306, 865.07, 483.553, 341.109] },
    Component { name: "Ethane", cas: "74-84-0", molar_mass: 30.06904, rho_c: 6.87085454, t_c: 305.322, residual: &ETHANE, n0: [36.73005938, -23639.65301, 4.00263, 4.33939, 1.23722, 13.1974, -6.01989], theta: [559.314, 223.284, 1031.38, 1071.29] },
    Component { name: "Propane", cas: "74-98-6", molar_mass: 44.09562, rho_c: 5.000043088, t_c: 369.825, residual: &PROPANE, n0: [44.70909619, -31236.63551, 4.02939, 6.60569, 3.197, 19.1921, -8.37267], theta: [479.856, 200.893, 955.312, 1027.29] },
    Component { name: "Isobutane", cas: "75-28-5", molar_mass: 58.1222, rho_c: 3.86014294, t_c: 407.817, residual: &ISOBUTANE, n0: [34.30180349, -38525.50276, 4.06714, 8.97575, 5.25156, 25.1423, 16.1388], theta: [438.27, 198.018, 1905.02, 893.765] },
    Component { name: "n-Butane", cas: "106-97-8", molar_mass: 58.1222, rho_c: 3.920016792, t_c: 425.125, residual: &N_BUTANE, n0: [36.53237783, -38957.80933, 4.33944, 9.44893, 6.89406, 24.4618, 14.7824], theta: [468.27, 183.636, 1914.1, 903.185] },
    Component { name: "Isopentane", cas: "78-78-4", molar_mass: 72.14878, rho_c: 3.271, t_c: 460.35, residual: &ISOPENTANE, n0: [43.17218626, -51198.30946, 4.0, 11.7618, 20.1101, 33.1688, 0.0], theta: [292.503, 910.237, 1919.37, 0.0] },
    Component { name: "n-Pentane", cas: "109-66-0", molar_mass: 72.14878, rho_c: 3.215577588, t_c: 469.7, residual: &N_PENTANE, n0: [42.67837089, -45215.83, 4.0, 8.95043, 21.836, 33.4032, 0.0], theta: [178.67, 840.538, 1774.25, 0.0] },
    Component { name: "n-Hexane", cas: "110-54-3", molar_mass: 86.17536, rho_c: 2.705877875, t_c: 507.82, residual: &N_HEXANE, n0: [46.99717188, -52746.83318, 4.0, 11.6977, 26.8142, 38.6164, 0.0], theta: [182.326, 859.207, 1826.59, 0.0] },
    Component { name: "n-Heptane", cas: "142-82-5", molar_mass: 100.20194, rho_c: 2.315324434, t_c: 540.13, residual: &N_HEPTANE, n0: [52.07631631, -57104.81056, 4.0, 13.7266, 30.4707, 43.5561, 0.0], theta: [169.789, 836.195, 1760.46, 0.0] },
    Component { name: "n-Octane", cas: "111-65-9", molar_mass: 114.22852, rho_c: 2.056404127, t_c: 569.32, residual: &N_OCTANE, n0: [57.25830934, -60546.76385, 4.0, 15.6865, 33.8029, 48.1731, 0.0], theta: [158.922, 815.064, 1693.07, 0.0] },
    Component { name: "n-Nonane", cas: "111-84-2", molar_mass: 128.2551, rho_c: 1.81, t_c: 594.55, residual: &N_NONANE, n0: [62.09646901, -66600.12837, 4.0, 18.0241, 38.1235, 53.3415, 0.0], theta: [156.854, 814.882, 1693.79, 0.0] },
    Component { name: "n-Decane", cas: "124-18-5", molar_mass: 142.28168, rho_c: 1.64, t_c: 617.7, residual: &N_DECANE, n0: [65.93909154, -74131.45483, 4.0, 21.0069, 43.4931, 58.3657, 0.0], theta: [164.947, 836.264, 1750.24, 0.0] },
    Component { name: "Hydrogen", cas: "1333-74-0", molar_mass: 2.01588, rho_c: 14.94, t_c: 33.19, residual: &HYDROGEN, n0: [13.07520288, -5836.943696, 2.47906, 0.95806, 0.45444, 1.56039, -1.3756], theta: [228.734, 326.843, 1651.71, 1671.69] },
    Component { name: "Oxygen", cas: "7782-44-7", molar_mass: 31.9988, rho_c: 13.63, t_c: 154.595, residual: &OXYGEN, n0: [16.8017173, -2318.32269, 3.50146, 1.07558, 1.01334, 0.0, 0.0], theta: [2235.71, 1116.69, 0.0, 0.0] },
    Component { name: "Carbon monoxide", cas: "630-08-0", molar_mass: 28.0101, rho_c: 10.85, t_c: 132.86, residual: &CARBON_MONOXIDE, n0: [17.45786899, -2635.244116, 3.50055, 1.02865, 0.00493, 0.0, 0.0], theta: [1550.45, 704.525, 0.0, 0.0] },
    Component { name: "Water", cas: "7732-18-5", molar_mass: 18.01528, rho_c: 17.87371609, t_c: 647.096, residual: &WATER, n0: [21.57882705, -7766.733078, 4.00392, 0.01059, 0.98763, 3.06904, 0.0], theta: [268.795, 1141.41, 2507.37, 0.0] },
    Component { name: "Hydrogen sulfide", cas: "7783-06-4", molar_mass: 34.08088, rho_c: 10.19, t_c: 373.1, residual: &HYDROGEN_SULFIDE, n0: [21.5830944, -6069.035869, 4.0, 3.11942, 1.00243, 0.0, 0.0], theta: [1833.63, 847.181, 0.0, 0.0] },
    Component { name: "Helium", cas: "7440-59-7", molar_mass: 4.002602, rho_c: 17.399, t_c: 5.1953, residual: &HELIUM, n0: [10.04639507, -745.375, 2.5, 0.0, 0.0, 0.0, 0.0], theta: [0.0, 0.0, 0.0, 0.0] },
    Component { name: "Argon", cas: "7440-37-1", molar_mass: 39.948, rho_c: 13.407429659, t_c: 150.687, residual: &ARGON, n0: [10.04639507, -745.375, 2.5, 0.0, 0.0, 0.0, 0.0], theta: [0.0, 0.0, 0.0, 0.0] },
];

/// Model 1, Methane-Ethane: 2 polynomial and 10 exponential terms.
static MODEL_1: [Term; 12] = [
    Term { n: -0.00080926050298746, d: 3.0, t: 0.65, ..ZERO },
    Term { n: -0.00075381925080059, d: 4.0, t: 1.55, ..ZERO },
    Term { n: -0.041618768891219, d: 1.0, t: 3.1, eta: 1.0, epsilon: 0.5, beta: 1.0, gamma: 0.5, ..ZERO },
    Term { n: -0.23452173681569, d: 2.0, t: 5.9, eta: 1.0, epsilon: 0.5, beta: 1.0, gamma: 0.5, ..ZERO },
    Term { n: 0.14003840584586, d: 2.0, t: 7.05, eta: 1.0, epsilon: 0.5, beta: 1.0, gamma: 0.5, ..ZERO },
    Term { n: 0.063281744807738, d: 2.0, t: 3.35, eta: 0.875, epsilon: 0.5, beta: 1.25, gamma: 0.5, ..ZERO },
    Term { n: -0.034660425848809, d: 2.0, t: 1.2, eta: 0.75, epsilon: 0.5, beta: 1.5, gamma: 0.5, ..ZERO },
    Term { n: -0.23918747334251, d: 2.0, t: 5.8, eta: 0.5, epsilon: 0.5, beta: 2.0, gamma: 0.5, ..ZERO },
    Term { n: 0.0019855255066891, d: 2.0, t: 2.7, eta: 0.0, epsilon: 0.5, beta: 3.0, gamma: 0.5, ..ZERO },
    Term { n: 6.1777746171555, d: 3.0, t: 0.45, eta: 0.0, epsilon: 0.5, beta: 3.0, gamma: 0.5, ..ZERO },
    Term { n: -6.9575358271105, d: 3.0, t: 0.55, eta: 0.0, epsilon: 0.5, beta: 3.0, gamma: 0.5, ..ZERO },
    Term { n: 1.0630185306388, d: 3.0, t: 1.95, eta: 0.0, epsilon: 0.5, beta: 3.0, gamma: 0.5, ..ZERO },
];

/// Model 2, Methane-Propane: 5 polynomial and 4 exponential terms.
static MODEL_2: [Term; 9] = [
    Term { n: 0.013746429958576, d: 3.0, t: 1.85, ..ZERO },
    Term { n: -0.0074425012129552, d: 3.0, t: 3.95, ..ZERO },
    Term { n: -0.0045516600213685, d: 4.0, t: 0.0, ..ZERO },
    Term { n: -0.0054546603350237, d: 4.0, t: 1.85, ..ZERO },
    Term { n: 0.0023682016824471, d: 4.0, t: 3.85, ..ZERO },
    Term { n: 0.18007763721438, d: 1.0, t: 5.25, eta: 0.25, epsilon: 0.5, beta: 0.75, gamma: 0.5, ..ZERO },
    Term { n: -0.44773942932486, d: 1.0, t: 3.85, eta: 0.25, epsilon: 0.5, beta: 1.0, gamma: 0.5, ..ZERO },
    Term { n: 0.0193273748882, d: 1.0, t: 0.2, eta: 0.0, epsilon: 0.5, beta: 2.0, gamma: 0.5, ..ZERO },
    Term { n: -0.30632197804624, d: 2.0, t: 6.5, eta: 0.0, epsilon: 0.5, beta: 3.0, gamma: 0.5, ..ZERO },
];

/// Model 3, Methane-Nitrogen: 2 polynomial and 7 exponential terms.
static MODEL_3: [Term; 9] = [
    Term { n: -0.0098038985517335, d: 1.0, t: 0.0, ..ZERO },
    Term { n: 0.00042487270143005, d: 4.0, t: 1.85, ..ZERO },
    Term { n: -0.034800214576142, d: 1.0, t: 7.85, eta: 1.0, epsilon: 0.5, beta: 1.0, gamma: 0.5, ..ZERO },
    Term { n: -0.13333813013896, d: 2.0, t: 5.4, eta: 1.0, epsilon: 0.5, beta: 1.0, gamma: 0.5, ..ZERO },
    Term { n: -0.011993694974627, d: 2.0, t: 0.0, eta: 0.25, epsilon: 0.5, beta: 2.5, gamma: 0.5, ..ZERO },
    Term { n: 0.069243379775168, d: 2.0, t: 0.75, eta: 0.0, epsilon: 0.5, beta: 3.0, gamma: 0.5, ..ZERO },
    Term { n: -0.31022508148249, d: 2.0, t: 2.8, eta: 0.0, epsilon: 0.5, beta: 3.0, gamma: 0.5, ..ZERO },
    Term { n: 0.24495491753226, d: 2.0, t: 4.45, eta: 0.0, epsilon: 0.5, beta: 3.0, gamma: 0.5, ..ZERO },
    Term { n: 0.22369816716981, d: 3.0, t: 4.25, eta: 0.0, epsilon: 0.5, beta: 3.0, gamma: 0.5, ..ZERO },
];

/// Model 4, Methane-Carbon dioxide: 3 polynomial and 3 exponential terms.
static MODEL_4: [Term; 6] = [
    Term { n: -0.10859387354942, d: 1.0, t: 2.6, ..ZERO },
    Term { n: 0.080228576727389, d: 2.0, t: 1.95, ..ZERO },
    Term { n: -0.0093303985115717, d: 3.0, t: 0.0, ..ZERO },
    Term { n: 0.040989274005848, d: 1.0, t: 3.95, eta: 1.0, epsilon: 0.5, beta: 1.0, gamma: 0.5, ..ZERO },
    Term { n: -0.24338019772494, d: 2.0, t: 7.95, eta: 0.5, epsilon: 0.5, beta: 2.0, gamma: 0.5, ..ZERO },
    Term { n: 0.23855347281124, d: 3.0, t: 8.0, eta: 0.0, epsilon: 0.5, beta: 3.0, gamma: 0.5, ..ZERO },
];

/// Model 5, Nitrogen-Carbon dioxide: 2 polynomial and 4 exponential terms.
static MODEL_5: [Term; 6] = [
    Term { n: 0.28661625028399, d: 2.0, t: 1.85, ..ZERO },
    Term { n: -0.10919833861247, d: 3.0, t: 1.4, ..ZERO },
    Term { n: -1.137403208227, d: 1.0, t: 3.2, eta: 0.25, epsilon: 0.5, beta: 0.75, gamma: 0.5, ..ZERO },
    Term { n: 0.76580544237358, d: 1.0, t: 2.5, eta: 0.25, epsilon: 0.5, beta: 1.0, gamma: 0.5, ..ZERO },
    Term { n: 0.0042638000926819, d: 1.0, t: 8.0, eta: 0.0, epsilon: 0.5, beta: 2.0, gamma: 0.5, ..ZERO },
    Term { n: 0.17673538204534, d: 2.0, t: 3.75, eta: 0.0, epsilon: 0.5, beta: 3.0, gamma: 0.5, ..ZERO },
];

/// Model 6, Nitrogen-Ethane: 3 polynomial and 3 exponential terms.
static MODEL_6: [Term; 6] = [
    Term { n: -0.47376518126608, d: 2.0, t: 0.0, ..ZERO },
    Term { n: 0.48961193461001, d: 2.0, t: 0.05, ..ZERO },
    Term { n: -0.0057011062090535, d: 3.0, t: 0.0, ..ZERO },
    Term { n: -0.1996682004132, d: 1.0, t: 3.65, eta: 1.0, epsilon: 0.5, beta: 1.0, gamma: 0.5, ..ZERO },
    Term { n: -0.69411103101723, d: 2.0, t: 4.9, eta: 1.0, epsilon: 0.5, beta: 1.0, gamma: 0.5, ..ZERO },
    Term { n: 0.69226192739021, d: 2.0, t: 4.45, eta: 0.875, epsilon: 0.5, beta: 1.25, gamma: 0.5, ..ZERO },
];

/// Model 7, Methane-Hydrogen: 4 polynomial and 0 exponential terms.
static MODEL_7: [Term; 4] = [
    Term { n: -0.25157134971934, d: 1.0, t: 2.0, ..ZERO },
    Term { n: -0.0062203841111983, d: 3.0, t: -1.0, ..ZERO },
    Term { n: 0.088850315184396, d: 3.0, t: 1.75, ..ZERO },
    Term { n: -0.035592212573239, d: 4.0, t: 1.4, ..ZERO },
];

/// Model 10, Generalized: 10 polynomial and 0 exponential terms.
static MODEL_10: [Term; 10] = [
    Term { n: 2.5574776844118, d: 1.0, t: 1.0, ..ZERO },
    Term { n: -7.9846357136353, d: 1.0, t: 1.55, ..ZERO },
    Term { n: 4.7859131465806, d: 1.0, t: 1.7, ..ZERO },
    Term { n: -0.73265392369587, d: 2.0, t: 0.25, ..ZERO },
    Term { n: 1.3805471345312, d: 2.0, t: 1.35, ..ZERO },
    Term { n: 0.28349603476365, d: 3.0, t: 0.0, ..ZERO },
    Term { n: -0.49087385940425, d: 3.0, t: 1.25, ..ZERO },
    Term { n: -0.10291888921447, d: 4.0, t: 0.0, ..ZERO },
    Term { n: 0.11836314681968, d: 4.0, t: 0.7, ..ZERO },
    Term { n: 5.5527385721943e-05, d: 4.0, t: 5.4, ..ZERO },
];

/// The departure functions by AGA8's model number: 1 to 7 binary-specific, 10 the generalized one.
pub(crate) static DEPARTURES: [(usize, DepartureData); 8] = [
    (1, DepartureData { name: "Methane-Ethane", aliases: &[], source: SOURCE, kind: Kind::Gerg2008, power: 2, terms: &MODEL_1 }),
    (2, DepartureData { name: "Methane-Propane", aliases: &[], source: SOURCE, kind: Kind::Gerg2008, power: 5, terms: &MODEL_2 }),
    (3, DepartureData { name: "Methane-Nitrogen", aliases: &[], source: SOURCE, kind: Kind::Gerg2008, power: 2, terms: &MODEL_3 }),
    (4, DepartureData { name: "Methane-Carbon dioxide", aliases: &[], source: SOURCE, kind: Kind::Gerg2008, power: 3, terms: &MODEL_4 }),
    (5, DepartureData { name: "Nitrogen-Carbon dioxide", aliases: &[], source: SOURCE, kind: Kind::Gerg2008, power: 2, terms: &MODEL_5 }),
    (6, DepartureData { name: "Nitrogen-Ethane", aliases: &[], source: SOURCE, kind: Kind::Gerg2008, power: 3, terms: &MODEL_6 }),
    (7, DepartureData { name: "Methane-Hydrogen", aliases: &[], source: SOURCE, kind: Kind::Gerg2008, power: 4, terms: &MODEL_7 }),
    (10, DepartureData { name: "Generalized", aliases: &[], source: SOURCE, kind: Kind::Gerg2008, power: 10, terms: &MODEL_10 }),
];

/// The 210 binaries, i < j in AGA8's order (indices from 0).
pub(crate) static BINARIES: [Binary; 210] = [
    Binary { i: 0, j: 1, beta_v: 0.998721377, gamma_v: 1.013950311, beta_t: 0.99809883, gamma_t: 0.979273013, f: 1.0, model: Some(3) }, // Methane-Nitrogen
    Binary { i: 0, j: 2, beta_v: 0.999518072, gamma_v: 1.002806594, beta_t: 1.02262449, gamma_t: 0.975665369, f: 1.0, model: Some(4) }, // Methane-Carbon dioxide
    Binary { i: 0, j: 3, beta_v: 0.997547866, gamma_v: 1.006617867, beta_t: 0.996336508, gamma_t: 1.049707697, f: 1.0, model: Some(1) }, // Methane-Ethane
    Binary { i: 0, j: 4, beta_v: 1.00482707, gamma_v: 1.038470657, beta_t: 0.989680305, gamma_t: 1.098655531, f: 1.0, model: Some(2) }, // Methane-Propane
    Binary { i: 0, j: 5, beta_v: 1.011240388, gamma_v: 1.054319053, beta_t: 0.980315756, gamma_t: 1.161117729, f: 0.771035405688, model: Some(10) }, // Methane-Isobutane
    Binary { i: 0, j: 6, beta_v: 0.979105972, gamma_v: 1.045375122, beta_t: 0.99417491, gamma_t: 1.171607691, f: 1.0, model: Some(10) }, // Methane-n-Butane
    Binary { i: 0, j: 7, beta_v: 1.0, gamma_v: 1.343685343, beta_t: 1.0, gamma_t: 1.188899743, f: 0.0, model: None }, // Methane-Isopentane
    Binary { i: 0, j: 8, beta_v: 0.94833012, gamma_v: 1.124508039, beta_t: 0.992127525, gamma_t: 1.249173968, f: 0.0, model: None }, // Methane-n-Pentane
    Binary { i: 0, j: 9, beta_v: 0.958015294, gamma_v: 1.052643846, beta_t: 0.981844797, gamma_t: 1.330570181, f: 0.0, model: None }, // Methane-n-Hexane
    Binary { i: 0, j: 10, beta_v: 0.962050831, gamma_v: 1.156655935, beta_t: 0.977431529, gamma_t: 1.379850328, f: 0.0, model: None }, // Methane-n-Heptane
    Binary { i: 0, j: 11, beta_v: 0.994740603, gamma_v: 1.116549372, beta_t: 0.957473785, gamma_t: 1.449245409, f: 0.0, model: None }, // Methane-n-Octane
    Binary { i: 0, j: 12, beta_v: 1.002852287, gamma_v: 1.141895355, beta_t: 0.947716769, gamma_t: 1.528532478, f: 0.0, model: None }, // Methane-n-Nonane
    Binary { i: 0, j: 13, beta_v: 1.033086292, gamma_v: 1.146089637, beta_t: 0.937777823, gamma_t: 1.568231489, f: 0.0, model: None }, // Methane-n-Decane
    Binary { i: 0, j: 14, beta_v: 1.0, gamma_v: 1.018702573, beta_t: 1.0, gamma_t: 1.352643115, f: 1.0, model: Some(7) }, // Methane-Hydrogen
    Binary { i: 0, j: 15, beta_v: 1.0, gamma_v: 1.0, beta_t: 1.0, gamma_t: 0.95, f: 0.0, model: None }, // Methane-Oxygen
    Binary { i: 0, j: 16, beta_v: 0.997340772, gamma_v: 1.006102927, beta_t: 0.987411732, gamma_t: 0.987473033, f: 0.0, model: None }, // Methane-Carbon monoxide
    Binary { i: 0, j: 17, beta_v: 1.012783169, gamma_v: 1.585018334, beta_t: 1.063333913, gamma_t: 0.775810513, f: 0.0, model: None }, // Methane-Water
    Binary { i: 0, j: 18, beta_v: 1.012599087, gamma_v: 1.040161207, beta_t: 1.011090031, gamma_t: 0.961155729, f: 0.0, model: None }, // Methane-Hydrogen sulfide
    Binary { i: 0, j: 19, beta_v: 1.0, gamma_v: 0.881405683, beta_t: 1.0, gamma_t: 3.159776855, f: 0.0, model: None }, // Methane-Helium
    Binary { i: 0, j: 20, beta_v: 1.034630259, gamma_v: 1.014678542, beta_t: 0.990954281, gamma_t: 0.989843388, f: 0.0, model: None }, // Methane-Argon
    Binary { i: 1, j: 2, beta_v: 0.977794634, gamma_v: 1.047578256, beta_t: 1.005894529, gamma_t: 1.107654104, f: 1.0, model: Some(5) }, // Nitrogen-Carbon dioxide
    Binary { i: 1, j: 3, beta_v: 0.978880168, gamma_v: 1.042352891, beta_t: 1.007671428, gamma_t: 1.098650964, f: 1.0, model: Some(6) }, // Nitrogen-Ethane
    Binary { i: 1, j: 4, beta_v: 0.974424681, gamma_v: 1.081025408, beta_t: 1.002677329, gamma_t: 1.201264026, f: 0.0, model: None }, // Nitrogen-Propane
    Binary { i: 1, j: 5, beta_v: 0.98641583, gamma_v: 1.100576129, beta_t: 0.99286813, gamma_t: 1.284462634, f: 0.0, model: None }, // Nitrogen-Isobutane
    Binary { i: 1, j: 6, beta_v: 0.99608261, gamma_v: 1.146949309, beta_t: 0.994515234, gamma_t: 1.304886838, f: 0.0, model: None }, // Nitrogen-n-Butane
    Binary { i: 1, j: 7, beta_v: 1.0, gamma_v: 1.154135439, beta_t: 1.0, gamma_t: 1.38177077, f: 0.0, model: None }, // Nitrogen-Isopentane
    Binary { i: 1, j: 8, beta_v: 1.0, gamma_v: 1.078877166, beta_t: 1.0, gamma_t: 1.419029041, f: 0.0, model: None }, // Nitrogen-n-Pentane
    Binary { i: 1, j: 9, beta_v: 1.0, gamma_v: 1.195952177, beta_t: 1.0, gamma_t: 1.472607971, f: 0.0, model: None }, // Nitrogen-n-Hexane
    Binary { i: 1, j: 10, beta_v: 1.0, gamma_v: 1.40455409, beta_t: 1.0, gamma_t: 1.520975334, f: 0.0, model: None }, // Nitrogen-n-Heptane
    Binary { i: 1, j: 11, beta_v: 1.0, gamma_v: 1.186067025, beta_t: 1.0, gamma_t: 1.733280051, f: 0.0, model: None }, // Nitrogen-n-Octane
    Binary { i: 1, j: 12, beta_v: 1.0, gamma_v: 1.100405929, beta_t: 0.95637945, gamma_t: 1.749119996, f: 0.0, model: None }, // Nitrogen-n-Nonane
    Binary { i: 1, j: 13, beta_v: 1.0, gamma_v: 1.0, beta_t: 0.957934447, gamma_t: 1.822157123, f: 0.0, model: None }, // Nitrogen-n-Decane
    Binary { i: 1, j: 14, beta_v: 0.972532065, gamma_v: 0.970115357, beta_t: 0.946134337, gamma_t: 1.175696583, f: 0.0, model: None }, // Nitrogen-Hydrogen
    Binary { i: 1, j: 15, beta_v: 0.99952177, gamma_v: 0.997082328, beta_t: 0.997190589, gamma_t: 0.995157044, f: 0.0, model: None }, // Nitrogen-Oxygen
    Binary { i: 1, j: 16, beta_v: 1.0, gamma_v: 1.008690943, beta_t: 1.0, gamma_t: 0.993425388, f: 0.0, model: None }, // Nitrogen-Carbon monoxide
    Binary { i: 1, j: 17, beta_v: 1.0, gamma_v: 1.094749685, beta_t: 1.0, gamma_t: 0.968808467, f: 0.0, model: None }, // Nitrogen-Water
    Binary { i: 1, j: 18, beta_v: 0.910394249, gamma_v: 1.256844157, beta_t: 1.004692366, gamma_t: 0.9601742, f: 0.0, model: None }, // Nitrogen-Hydrogen sulfide
    Binary { i: 1, j: 19, beta_v: 0.969501055, gamma_v: 0.932629867, beta_t: 0.692868765, gamma_t: 1.47183158, f: 0.0, model: None }, // Nitrogen-Helium
    Binary { i: 1, j: 20, beta_v: 1.004166412, gamma_v: 1.002212182, beta_t: 0.999069843, gamma_t: 0.990034831, f: 0.0, model: None }, // Nitrogen-Argon
    Binary { i: 2, j: 3, beta_v: 1.002525718, gamma_v: 1.032876701, beta_t: 1.013871147, gamma_t: 0.90094953, f: 0.0, model: None }, // Carbon dioxide-Ethane
    Binary { i: 2, j: 4, beta_v: 0.996898004, gamma_v: 1.047596298, beta_t: 1.033620538, gamma_t: 0.908772477, f: 0.0, model: None }, // Carbon dioxide-Propane
    Binary { i: 2, j: 5, beta_v: 1.076551882, gamma_v: 1.081909003, beta_t: 1.023339824, gamma_t: 0.929982936, f: 0.0, model: None }, // Carbon dioxide-Isobutane
    Binary { i: 2, j: 6, beta_v: 1.174760923, gamma_v: 1.222437324, beta_t: 1.018171004, gamma_t: 0.911498231, f: 0.0, model: None }, // Carbon dioxide-n-Butane
    Binary { i: 2, j: 7, beta_v: 1.060793104, gamma_v: 1.116793198, beta_t: 1.019180957, gamma_t: 0.961218039, f: 0.0, model: None }, // Carbon dioxide-Isopentane
    Binary { i: 2, j: 8, beta_v: 1.024311498, gamma_v: 1.068406078, beta_t: 1.027000795, gamma_t: 0.979217302, f: 0.0, model: None }, // Carbon dioxide-n-Pentane
    Binary { i: 2, j: 9, beta_v: 1.0, gamma_v: 0.851343711, beta_t: 1.0, gamma_t: 1.038675574, f: 0.0, model: None }, // Carbon dioxide-n-Hexane
    Binary { i: 2, j: 10, beta_v: 1.205469976, gamma_v: 1.164585914, beta_t: 1.011806317, gamma_t: 1.046169823, f: 0.0, model: None }, // Carbon dioxide-n-Heptane
    Binary { i: 2, j: 11, beta_v: 1.026169373, gamma_v: 1.104043935, beta_t: 1.02969078, gamma_t: 1.074455386, f: 0.0, model: None }, // Carbon dioxide-n-Octane
    Binary { i: 2, j: 12, beta_v: 1.0, gamma_v: 0.973386152, beta_t: 1.00768862, gamma_t: 1.140671202, f: 0.0, model: None }, // Carbon dioxide-n-Nonane
    Binary { i: 2, j: 13, beta_v: 1.000151132, gamma_v: 1.183394668, beta_t: 1.02002879, gamma_t: 1.145512213, f: 0.0, model: None }, // Carbon dioxide-n-Decane
    Binary { i: 2, j: 14, beta_v: 0.904142159, gamma_v: 1.15279255, beta_t: 0.942320195, gamma_t: 1.782924792, f: 0.0, model: None }, // Carbon dioxide-Hydrogen
    Binary { i: 2, j: 15, beta_v: 1.0, gamma_v: 1.0, beta_t: 1.0, gamma_t: 1.0, f: 0.0, model: None }, // Carbon dioxide-Oxygen
    Binary { i: 2, j: 16, beta_v: 1.0, gamma_v: 1.0, beta_t: 1.0, gamma_t: 1.0, f: 0.0, model: None }, // Carbon dioxide-Carbon monoxide
    Binary { i: 2, j: 17, beta_v: 0.949055959, gamma_v: 1.542328793, beta_t: 0.997372205, gamma_t: 0.775453996, f: 0.0, model: None }, // Carbon dioxide-Water
    Binary { i: 2, j: 18, beta_v: 0.906630564, gamma_v: 1.024085837, beta_t: 1.016034583, gamma_t: 0.92601888, f: 0.0, model: None }, // Carbon dioxide-Hydrogen sulfide
    Binary { i: 2, j: 19, beta_v: 0.846647561, gamma_v: 0.864141549, beta_t: 0.76837763, gamma_t: 3.207456948, f: 0.0, model: None }, // Carbon dioxide-Helium
    Binary { i: 2, j: 20, beta_v: 1.008392428, gamma_v: 1.029205465, beta_t: 0.996512863, gamma_t: 1.050971635, f: 0.0, model: None }, // Carbon dioxide-Argon
    Binary { i: 3, j: 4, beta_v: 0.997607277, gamma_v: 1.00303472, beta_t: 0.996199694, gamma_t: 1.01473019, f: 0.13042476515, model: Some(10) }, // Ethane-Propane
    Binary { i: 3, j: 5, beta_v: 1.0, gamma_v: 1.006616886, beta_t: 1.0, gamma_t: 1.033283811, f: 0.260632376098, model: Some(10) }, // Ethane-Isobutane
    Binary { i: 3, j: 6, beta_v: 0.999157205, gamma_v: 1.006179146, beta_t: 0.999130554, gamma_t: 1.034832749, f: 0.281570073085, model: Some(10) }, // Ethane-n-Butane
    Binary { i: 3, j: 7, beta_v: 1.0, gamma_v: 1.045439935, beta_t: 1.0, gamma_t: 1.021150247, f: 0.0, model: None }, // Ethane-Isopentane
    Binary { i: 3, j: 8, beta_v: 0.993851009, gamma_v: 1.026085655, beta_t: 0.998688946, gamma_t: 1.066665676, f: 0.0, model: None }, // Ethane-n-Pentane
    Binary { i: 3, j: 9, beta_v: 1.0, gamma_v: 1.169701102, beta_t: 1.0, gamma_t: 1.092177796, f: 0.0, model: None }, // Ethane-n-Hexane
    Binary { i: 3, j: 10, beta_v: 1.0, gamma_v: 1.057666085, beta_t: 1.0, gamma_t: 1.134532014, f: 0.0, model: None }, // Ethane-n-Heptane
    Binary { i: 3, j: 11, beta_v: 1.007469726, gamma_v: 1.071917985, beta_t: 0.984068272, gamma_t: 1.168636194, f: 0.0, model: None }, // Ethane-n-Octane
    Binary { i: 3, j: 12, beta_v: 1.0, gamma_v: 1.14353473, beta_t: 1.0, gamma_t: 1.05603303, f: 0.0, model: None }, // Ethane-n-Nonane
    Binary { i: 3, j: 13, beta_v: 0.995676258, gamma_v: 1.098361281, beta_t: 0.970918061, gamma_t: 1.237191558, f: 0.0, model: None }, // Ethane-n-Decane
    Binary { i: 3, j: 14, beta_v: 0.925367171, gamma_v: 1.10607204, beta_t: 0.932969831, gamma_t: 1.902008495, f: 0.0, model: None }, // Ethane-Hydrogen
    Binary { i: 3, j: 15, beta_v: 1.0, gamma_v: 1.0, beta_t: 1.0, gamma_t: 1.0, f: 0.0, model: None }, // Ethane-Oxygen
    Binary { i: 3, j: 16, beta_v: 1.0, gamma_v: 1.201417898, beta_t: 1.0, gamma_t: 1.069224728, f: 0.0, model: None }, // Ethane-Carbon monoxide
    Binary { i: 3, j: 17, beta_v: 1.0, gamma_v: 1.0, beta_t: 1.0, gamma_t: 1.0, f: 0.0, model: None }, // Ethane-Water
    Binary { i: 3, j: 18, beta_v: 1.010817909, gamma_v: 1.030988277, beta_t: 0.990197354, gamma_t: 0.90273666, f: 0.0, model: None }, // Ethane-Hydrogen sulfide
    Binary { i: 3, j: 19, beta_v: 1.0, gamma_v: 1.0, beta_t: 1.0, gamma_t: 1.0, f: 0.0, model: None }, // Ethane-Helium
    Binary { i: 3, j: 20, beta_v: 1.0, gamma_v: 1.0, beta_t: 1.0, gamma_t: 1.0, f: 0.0, model: None }, // Ethane-Argon
    Binary { i: 4, j: 5, beta_v: 0.999243146, gamma_v: 1.001156119, beta_t: 0.998012298, gamma_t: 1.005250774, f: -0.0551609771024, model: Some(10) }, // Propane-Isobutane
    Binary { i: 4, j: 6, beta_v: 0.999795868, gamma_v: 1.003264179, beta_t: 1.000310289, gamma_t: 1.007392782, f: 0.0312572600489, model: Some(10) }, // Propane-n-Butane
    Binary { i: 4, j: 7, beta_v: 1.040459289, gamma_v: 0.999432118, beta_t: 0.994364425, gamma_t: 1.0032695, f: 0.0, model: None }, // Propane-Isopentane
    Binary { i: 4, j: 8, beta_v: 1.044919431, gamma_v: 1.019921513, beta_t: 0.996484021, gamma_t: 1.008344412, f: 0.0, model: None }, // Propane-n-Pentane
    Binary { i: 4, j: 9, beta_v: 1.0, gamma_v: 1.057872566, beta_t: 1.0, gamma_t: 1.025657518, f: 0.0, model: None }, // Propane-n-Hexane
    Binary { i: 4, j: 10, beta_v: 1.0, gamma_v: 1.079648053, beta_t: 1.0, gamma_t: 1.050044169, f: 0.0, model: None }, // Propane-n-Heptane
    Binary { i: 4, j: 11, beta_v: 1.0, gamma_v: 1.102764612, beta_t: 1.0, gamma_t: 1.063694129, f: 0.0, model: None }, // Propane-n-Octane
    Binary { i: 4, j: 12, beta_v: 1.0, gamma_v: 1.199769134, beta_t: 1.0, gamma_t: 1.109973833, f: 0.0, model: None }, // Propane-n-Nonane
    Binary { i: 4, j: 13, beta_v: 0.984104227, gamma_v: 1.053040574, beta_t: 0.985331233, gamma_t: 1.140905252, f: 0.0, model: None }, // Propane-n-Decane
    Binary { i: 4, j: 14, beta_v: 1.0, gamma_v: 1.07400611, beta_t: 1.0, gamma_t: 2.308215191, f: 0.0, model: None }, // Propane-Hydrogen
    Binary { i: 4, j: 15, beta_v: 1.0, gamma_v: 1.0, beta_t: 1.0, gamma_t: 1.0, f: 0.0, model: None }, // Propane-Oxygen
    Binary { i: 4, j: 16, beta_v: 1.0, gamma_v: 1.108143673, beta_t: 1.0, gamma_t: 1.197564208, f: 0.0, model: None }, // Propane-Carbon monoxide
    Binary { i: 4, j: 17, beta_v: 1.0, gamma_v: 1.011759763, beta_t: 1.0, gamma_t: 0.600340961, f: 0.0, model: None }, // Propane-Water
    Binary { i: 4, j: 18, beta_v: 0.936811219, gamma_v: 1.010593999, beta_t: 0.992573556, gamma_t: 0.905829247, f: 0.0, model: None }, // Propane-Hydrogen sulfide
    Binary { i: 4, j: 19, beta_v: 1.0, gamma_v: 1.0, beta_t: 1.0, gamma_t: 1.0, f: 0.0, model: None }, // Propane-Helium
    Binary { i: 4, j: 20, beta_v: 1.0, gamma_v: 1.0, beta_t: 1.0, gamma_t: 1.0, f: 0.0, model: None }, // Propane-Argon
    Binary { i: 5, j: 6, beta_v: 0.999120311, gamma_v: 1.00041444, beta_t: 0.999922459, gamma_t: 1.001432824, f: -0.0551240293009, model: Some(10) }, // Isobutane-n-Butane
    Binary { i: 5, j: 7, beta_v: 1.0, gamma_v: 1.002284353, beta_t: 1.0, gamma_t: 1.001835788, f: 0.0, model: None }, // Isobutane-Isopentane
    Binary { i: 5, j: 8, beta_v: 1.0, gamma_v: 1.002779804, beta_t: 1.0, gamma_t: 1.002495889, f: 0.0, model: None }, // Isobutane-n-Pentane
    Binary { i: 5, j: 9, beta_v: 1.0, gamma_v: 1.010493989, beta_t: 1.0, gamma_t: 1.006018054, f: 0.0, model: None }, // Isobutane-n-Hexane
    Binary { i: 5, j: 10, beta_v: 1.0, gamma_v: 1.021668316, beta_t: 1.0, gamma_t: 1.00988576, f: 0.0, model: None }, // Isobutane-n-Heptane
    Binary { i: 5, j: 11, beta_v: 1.0, gamma_v: 1.032807063, beta_t: 1.0, gamma_t: 1.013945424, f: 0.0, model: None }, // Isobutane-n-Octane
    Binary { i: 5, j: 12, beta_v: 1.0, gamma_v: 1.047298475, beta_t: 1.0, gamma_t: 1.017817492, f: 0.0, model: None }, // Isobutane-n-Nonane
    Binary { i: 5, j: 13, beta_v: 1.0, gamma_v: 1.060243344, beta_t: 1.0, gamma_t: 1.021624748, f: 0.0, model: None }, // Isobutane-n-Decane
    Binary { i: 5, j: 14, beta_v: 1.0, gamma_v: 1.147595688, beta_t: 1.0, gamma_t: 1.895305393, f: 0.0, model: None }, // Isobutane-Hydrogen
    Binary { i: 5, j: 15, beta_v: 1.0, gamma_v: 1.0, beta_t: 1.0, gamma_t: 1.0, f: 0.0, model: None }, // Isobutane-Oxygen
    Binary { i: 5, j: 16, beta_v: 1.0, gamma_v: 1.087272232, beta_t: 1.0, gamma_t: 1.161390082, f: 0.0, model: None }, // Isobutane-Carbon monoxide
    Binary { i: 5, j: 17, beta_v: 1.0, gamma_v: 1.0, beta_t: 1.0, gamma_t: 1.0, f: 0.0, model: None }, // Isobutane-Water
    Binary { i: 5, j: 18, beta_v: 1.012994431, gamma_v: 0.988591117, beta_t: 0.974550548, gamma_t: 0.937130844, f: 0.0, model: None }, // Isobutane-Hydrogen sulfide
    Binary { i: 5, j: 19, beta_v: 1.0, gamma_v: 1.0, beta_t: 1.0, gamma_t: 1.0, f: 0.0, model: None }, // Isobutane-Helium
    Binary { i: 5, j: 20, beta_v: 1.0, gamma_v: 1.0, beta_t: 1.0, gamma_t: 1.0, f: 0.0, model: None }, // Isobutane-Argon
    Binary { i: 6, j: 7, beta_v: 1.0, gamma_v: 1.002728434, beta_t: 1.0, gamma_t: 1.000792201, f: 0.0, model: None }, // n-Butane-Isopentane
    Binary { i: 6, j: 8, beta_v: 1.0, gamma_v: 1.01815965, beta_t: 1.0, gamma_t: 1.00214364, f: 0.0, model: None }, // n-Butane-n-Pentane
    Binary { i: 6, j: 9, beta_v: 1.0, gamma_v: 1.034995284, beta_t: 1.0, gamma_t: 1.00915706, f: 0.0, model: None }, // n-Butane-n-Hexane
    Binary { i: 6, j: 10, beta_v: 1.0, gamma_v: 1.019174227, beta_t: 1.0, gamma_t: 1.021283378, f: 0.0, model: None }, // n-Butane-n-Heptane
    Binary { i: 6, j: 11, beta_v: 1.0, gamma_v: 1.046905515, beta_t: 1.0, gamma_t: 1.033180106, f: 0.0, model: None }, // n-Butane-n-Octane
    Binary { i: 6, j: 12, beta_v: 1.0, gamma_v: 1.049219137, beta_t: 1.0, gamma_t: 1.014096448, f: 0.0, model: None }, // n-Butane-n-Nonane
    Binary { i: 6, j: 13, beta_v: 0.976951968, gamma_v: 1.027845529, beta_t: 0.993688386, gamma_t: 1.076466918, f: 0.0, model: None }, // n-Butane-n-Decane
    Binary { i: 6, j: 14, beta_v: 1.0, gamma_v: 1.232939523, beta_t: 1.0, gamma_t: 2.509259945, f: 0.0, model: None }, // n-Butane-Hydrogen
    Binary { i: 6, j: 15, beta_v: 1.0, gamma_v: 1.0, beta_t: 1.0, gamma_t: 1.0, f: 0.0, model: None }, // n-Butane-Oxygen
    Binary { i: 6, j: 16, beta_v: 1.0, gamma_v: 1.084740904, beta_t: 1.0, gamma_t: 1.173916162, f: 0.0, model: None }, // n-Butane-Carbon monoxide
    Binary { i: 6, j: 17, beta_v: 1.0, gamma_v: 1.223638763, beta_t: 1.0, gamma_t: 0.615512682, f: 0.0, model: None }, // n-Butane-Water
    Binary { i: 6, j: 18, beta_v: 0.908113163, gamma_v: 1.033366041, beta_t: 0.985962886, gamma_t: 0.926156602, f: 0.0, model: None }, // n-Butane-Hydrogen sulfide
    Binary { i: 6, j: 19, beta_v: 1.0, gamma_v: 1.0, beta_t: 1.0, gamma_t: 1.0, f: 0.0, model: None }, // n-Butane-Helium
    Binary { i: 6, j: 20, beta_v: 1.0, gamma_v: 1.214638734, beta_t: 1.0, gamma_t: 1.245039498, f: 0.0, model: None }, // n-Butane-Argon
    Binary { i: 7, j: 8, beta_v: 1.0, gamma_v: 1.000024335, beta_t: 1.0, gamma_t: 1.000050537, f: 0.0, model: None }, // Isopentane-n-Pentane
    Binary { i: 7, j: 9, beta_v: 1.0, gamma_v: 1.002995876, beta_t: 1.0, gamma_t: 1.001204174, f: 0.0, model: None }, // Isopentane-n-Hexane
    Binary { i: 7, j: 10, beta_v: 1.0, gamma_v: 1.009928206, beta_t: 1.0, gamma_t: 1.003194615, f: 0.0, model: None }, // Isopentane-n-Heptane
    Binary { i: 7, j: 11, beta_v: 1.0, gamma_v: 1.017880545, beta_t: 1.0, gamma_t: 1.00564748, f: 0.0, model: None }, // Isopentane-n-Octane
    Binary { i: 7, j: 12, beta_v: 1.0, gamma_v: 1.028994325, beta_t: 1.0, gamma_t: 1.008191499, f: 0.0, model: None }, // Isopentane-n-Nonane
    Binary { i: 7, j: 13, beta_v: 1.0, gamma_v: 1.039372957, beta_t: 1.0, gamma_t: 1.010825138, f: 0.0, model: None }, // Isopentane-n-Decane
    Binary { i: 7, j: 14, beta_v: 1.0, gamma_v: 1.184340443, beta_t: 1.0, gamma_t: 1.996386669, f: 0.0, model: None }, // Isopentane-Hydrogen
    Binary { i: 7, j: 15, beta_v: 1.0, gamma_v: 1.0, beta_t: 1.0, gamma_t: 1.0, f: 0.0, model: None }, // Isopentane-Oxygen
    Binary { i: 7, j: 16, beta_v: 1.0, gamma_v: 1.116694577, beta_t: 1.0, gamma_t: 1.199326059, f: 0.0, model: None }, // Isopentane-Carbon monoxide
    Binary { i: 7, j: 17, beta_v: 1.0, gamma_v: 1.0, beta_t: 1.0, gamma_t: 1.0, f: 0.0, model: None }, // Isopentane-Water
    Binary { i: 7, j: 18, beta_v: 1.0, gamma_v: 0.835763343, beta_t: 1.0, gamma_t: 0.982651529, f: 0.0, model: None }, // Isopentane-Hydrogen sulfide
    Binary { i: 7, j: 19, beta_v: 1.0, gamma_v: 1.0, beta_t: 1.0, gamma_t: 1.0, f: 0.0, model: None }, // Isopentane-Helium
    Binary { i: 7, j: 20, beta_v: 1.0, gamma_v: 1.0, beta_t: 1.0, gamma_t: 1.0, f: 0.0, model: None }, // Isopentane-Argon
    Binary { i: 8, j: 9, beta_v: 1.0, gamma_v: 1.002480637, beta_t: 1.0, gamma_t: 1.000761237, f: 0.0, model: None }, // n-Pentane-n-Hexane
    Binary { i: 8, j: 10, beta_v: 1.0, gamma_v: 1.008972412, beta_t: 1.0, gamma_t: 1.002441051, f: 0.0, model: None }, // n-Pentane-n-Heptane
    Binary { i: 8, j: 11, beta_v: 1.0, gamma_v: 1.069223964, beta_t: 1.0, gamma_t: 1.016422347, f: 0.0, model: None }, // n-Pentane-n-Octane
    Binary { i: 8, j: 12, beta_v: 1.0, gamma_v: 1.034910633, beta_t: 1.0, gamma_t: 1.103421755, f: 0.0, model: None }, // n-Pentane-n-Nonane
    Binary { i: 8, j: 13, beta_v: 1.0, gamma_v: 1.016370338, beta_t: 1.0, gamma_t: 1.049035838, f: 0.0, model: None }, // n-Pentane-n-Decane
    Binary { i: 8, j: 14, beta_v: 1.0, gamma_v: 1.188334783, beta_t: 1.0, gamma_t: 2.013859174, f: 0.0, model: None }, // n-Pentane-Hydrogen
    Binary { i: 8, j: 15, beta_v: 1.0, gamma_v: 1.0, beta_t: 1.0, gamma_t: 1.0, f: 0.0, model: None }, // n-Pentane-Oxygen
    Binary { i: 8, j: 16, beta_v: 1.0, gamma_v: 1.119954454, beta_t: 1.0, gamma_t: 1.206043295, f: 0.0, model: None }, // n-Pentane-Carbon monoxide
    Binary { i: 8, j: 17, beta_v: 1.0, gamma_v: 0.95667731, beta_t: 1.0, gamma_t: 0.447666011, f: 0.0, model: None }, // n-Pentane-Water
    Binary { i: 8, j: 18, beta_v: 0.984613203, gamma_v: 1.076539234, beta_t: 0.962006651, gamma_t: 0.959065662, f: 0.0, model: None }, // n-Pentane-Hydrogen sulfide
    Binary { i: 8, j: 19, beta_v: 1.0, gamma_v: 1.0, beta_t: 1.0, gamma_t: 1.0, f: 0.0, model: None }, // n-Pentane-Helium
    Binary { i: 8, j: 20, beta_v: 1.0, gamma_v: 1.0, beta_t: 1.0, gamma_t: 1.0, f: 0.0, model: None }, // n-Pentane-Argon
    Binary { i: 9, j: 10, beta_v: 1.0, gamma_v: 1.001508227, beta_t: 1.0, gamma_t: 0.999762786, f: 0.0, model: None }, // n-Hexane-n-Heptane
    Binary { i: 9, j: 11, beta_v: 1.0, gamma_v: 1.006268954, beta_t: 1.0, gamma_t: 1.001633952, f: 0.0, model: None }, // n-Hexane-n-Octane
    Binary { i: 9, j: 12, beta_v: 1.0, gamma_v: 1.02076168, beta_t: 1.0, gamma_t: 1.055369591, f: 0.0, model: None }, // n-Hexane-n-Nonane
    Binary { i: 9, j: 13, beta_v: 1.001516371, gamma_v: 1.013511439, beta_t: 0.99764101, gamma_t: 1.028939539, f: 0.0, model: None }, // n-Hexane-n-Decane
    Binary { i: 9, j: 14, beta_v: 1.0, gamma_v: 1.243461678, beta_t: 1.0, gamma_t: 3.021197546, f: 0.0, model: None }, // n-Hexane-Hydrogen
    Binary { i: 9, j: 15, beta_v: 1.0, gamma_v: 1.0, beta_t: 1.0, gamma_t: 1.0, f: 0.0, model: None }, // n-Hexane-Oxygen
    Binary { i: 9, j: 16, beta_v: 1.0, gamma_v: 1.155145836, beta_t: 1.0, gamma_t: 1.233272781, f: 0.0, model: None }, // n-Hexane-Carbon monoxide
    Binary { i: 9, j: 17, beta_v: 1.0, gamma_v: 1.170217596, beta_t: 1.0, gamma_t: 0.569681333, f: 0.0, model: None }, // n-Hexane-Water
    Binary { i: 9, j: 18, beta_v: 0.754473958, gamma_v: 1.339283552, beta_t: 0.985891113, gamma_t: 0.956075596, f: 0.0, model: None }, // n-Hexane-Hydrogen sulfide
    Binary { i: 9, j: 19, beta_v: 1.0, gamma_v: 1.0, beta_t: 1.0, gamma_t: 1.0, f: 0.0, model: None }, // n-Hexane-Helium
    Binary { i: 9, j: 20, beta_v: 1.0, gamma_v: 1.0, beta_t: 1.0, gamma_t: 1.0, f: 0.0, model: None }, // n-Hexane-Argon
    Binary { i: 10, j: 11, beta_v: 1.0, gamma_v: 1.006767176, beta_t: 1.0, gamma_t: 0.998793111, f: 0.0, model: None }, // n-Heptane-n-Octane
    Binary { i: 10, j: 12, beta_v: 1.0, gamma_v: 1.001370076, beta_t: 1.0, gamma_t: 1.001150096, f: 0.0, model: None }, // n-Heptane-n-Nonane
    Binary { i: 10, j: 13, beta_v: 1.0, gamma_v: 1.002972346, beta_t: 1.0, gamma_t: 1.002229938, f: 0.0, model: None }, // n-Heptane-n-Decane
    Binary { i: 10, j: 14, beta_v: 1.0, gamma_v: 1.159131722, beta_t: 1.0, gamma_t: 3.169143057, f: 0.0, model: None }, // n-Heptane-Hydrogen
    Binary { i: 10, j: 15, beta_v: 1.0, gamma_v: 1.0, beta_t: 1.0, gamma_t: 1.0, f: 0.0, model: None }, // n-Heptane-Oxygen
    Binary { i: 10, j: 16, beta_v: 1.0, gamma_v: 1.190354273, beta_t: 1.0, gamma_t: 1.256123503, f: 0.0, model: None }, // n-Heptane-Carbon monoxide
    Binary { i: 10, j: 17, beta_v: 1.0, gamma_v: 1.0, beta_t: 1.0, gamma_t: 1.0, f: 0.0, model: None }, // n-Heptane-Water
    Binary { i: 10, j: 18, beta_v: 0.828967164, gamma_v: 1.087956749, beta_t: 0.988937417, gamma_t: 1.013453092, f: 0.0, model: None }, // n-Heptane-Hydrogen sulfide
    Binary { i: 10, j: 19, beta_v: 1.0, gamma_v: 1.0, beta_t: 1.0, gamma_t: 1.0, f: 0.0, model: None }, // n-Heptane-Helium
    Binary { i: 10, j: 20, beta_v: 1.0, gamma_v: 1.0, beta_t: 1.0, gamma_t: 1.0, f: 0.0, model: None }, // n-Heptane-Argon
    Binary { i: 11, j: 12, beta_v: 1.0, gamma_v: 1.001357085, beta_t: 1.0, gamma_t: 1.000235044, f: 0.0, model: None }, // n-Octane-n-Nonane
    Binary { i: 11, j: 13, beta_v: 1.0, gamma_v: 1.002553544, beta_t: 1.0, gamma_t: 1.007186267, f: 0.0, model: None }, // n-Octane-n-Decane
    Binary { i: 11, j: 14, beta_v: 1.0, gamma_v: 1.305249405, beta_t: 1.0, gamma_t: 2.191555216, f: 0.0, model: None }, // n-Octane-Hydrogen
    Binary { i: 11, j: 15, beta_v: 1.0, gamma_v: 1.0, beta_t: 1.0, gamma_t: 1.0, f: 0.0, model: None }, // n-Octane-Oxygen
    Binary { i: 11, j: 16, beta_v: 1.0, gamma_v: 1.219206702, beta_t: 1.0, gamma_t: 1.276565536, f: 0.0, model: None }, // n-Octane-Carbon monoxide
    Binary { i: 11, j: 17, beta_v: 1.0, gamma_v: 0.599484191, beta_t: 1.0, gamma_t: 0.662072469, f: 0.0, model: None }, // n-Octane-Water
    Binary { i: 11, j: 18, beta_v: 1.0, gamma_v: 1.0, beta_t: 1.0, gamma_t: 1.0, f: 0.0, model: None }, // n-Octane-Hydrogen sulfide
    Binary { i: 11, j: 19, beta_v: 1.0, gamma_v: 1.0, beta_t: 1.0, gamma_t: 1.0, f: 0.0, model: None }, // n-Octane-Helium
    Binary { i: 11, j: 20, beta_v: 1.0, gamma_v: 1.0, beta_t: 1.0, gamma_t: 1.0, f: 0.0, model: None }, // n-Octane-Argon
    Binary { i: 12, j: 13, beta_v: 1.0, gamma_v: 1.00081052, beta_t: 1.0, gamma_t: 1.000182392, f: 0.0, model: None }, // n-Nonane-n-Decane
    Binary { i: 12, j: 14, beta_v: 1.0, gamma_v: 1.342647661, beta_t: 1.0, gamma_t: 2.23435404, f: 0.0, model: None }, // n-Nonane-Hydrogen
    Binary { i: 12, j: 15, beta_v: 1.0, gamma_v: 1.0, beta_t: 1.0, gamma_t: 1.0, f: 0.0, model: None }, // n-Nonane-Oxygen
    Binary { i: 12, j: 16, beta_v: 1.0, gamma_v: 1.252151449, beta_t: 1.0, gamma_t: 1.294070556, f: 0.0, model: None }, // n-Nonane-Carbon monoxide
    Binary { i: 12, j: 17, beta_v: 1.0, gamma_v: 1.0, beta_t: 1.0, gamma_t: 1.0, f: 0.0, model: None }, // n-Nonane-Water
    Binary { i: 12, j: 18, beta_v: 1.0, gamma_v: 1.082905109, beta_t: 1.0, gamma_t: 1.086557826, f: 0.0, model: None }, // n-Nonane-Hydrogen sulfide
    Binary { i: 12, j: 19, beta_v: 1.0, gamma_v: 1.0, beta_t: 1.0, gamma_t: 1.0, f: 0.0, model: None }, // n-Nonane-Helium
    Binary { i: 12, j: 20, beta_v: 1.0, gamma_v: 1.0, beta_t: 1.0, gamma_t: 1.0, f: 0.0, model: None }, // n-Nonane-Argon
    Binary { i: 13, j: 14, beta_v: 1.695358382, gamma_v: 1.120233729, beta_t: 1.064818089, gamma_t: 3.786003724, f: 0.0, model: None }, // n-Decane-Hydrogen
    Binary { i: 13, j: 15, beta_v: 1.0, gamma_v: 1.0, beta_t: 1.0, gamma_t: 1.0, f: 0.0, model: None }, // n-Decane-Oxygen
    Binary { i: 13, j: 16, beta_v: 1.0, gamma_v: 0.87018496, beta_t: 1.049594632, gamma_t: 1.803567587, f: 0.0, model: None }, // n-Decane-Carbon monoxide
    Binary { i: 13, j: 17, beta_v: 1.0, gamma_v: 0.551405318, beta_t: 0.897162268, gamma_t: 0.740416402, f: 0.0, model: None }, // n-Decane-Water
    Binary { i: 13, j: 18, beta_v: 0.975187766, gamma_v: 1.171714677, beta_t: 0.973091413, gamma_t: 1.103693489, f: 0.0, model: None }, // n-Decane-Hydrogen sulfide
    Binary { i: 13, j: 19, beta_v: 1.0, gamma_v: 1.0, beta_t: 1.0, gamma_t: 1.0, f: 0.0, model: None }, // n-Decane-Helium
    Binary { i: 13, j: 20, beta_v: 1.0, gamma_v: 1.0, beta_t: 1.0, gamma_t: 1.0, f: 0.0, model: None }, // n-Decane-Argon
    Binary { i: 14, j: 15, beta_v: 1.0, gamma_v: 1.0, beta_t: 1.0, gamma_t: 1.0, f: 0.0, model: None }, // Hydrogen-Oxygen
    Binary { i: 14, j: 16, beta_v: 1.0, gamma_v: 1.121416201, beta_t: 1.0, gamma_t: 1.377504607, f: 0.0, model: None }, // Hydrogen-Carbon monoxide
    Binary { i: 14, j: 17, beta_v: 1.0, gamma_v: 1.0, beta_t: 1.0, gamma_t: 1.0, f: 0.0, model: None }, // Hydrogen-Water
    Binary { i: 14, j: 18, beta_v: 1.0, gamma_v: 1.0, beta_t: 1.0, gamma_t: 1.0, f: 0.0, model: None }, // Hydrogen-Hydrogen sulfide
    Binary { i: 14, j: 19, beta_v: 1.0, gamma_v: 1.0, beta_t: 1.0, gamma_t: 1.0, f: 0.0, model: None }, // Hydrogen-Helium
    Binary { i: 14, j: 20, beta_v: 1.0, gamma_v: 1.0, beta_t: 1.0, gamma_t: 1.0, f: 0.0, model: None }, // Hydrogen-Argon
    Binary { i: 15, j: 16, beta_v: 1.0, gamma_v: 1.0, beta_t: 1.0, gamma_t: 1.0, f: 0.0, model: None }, // Oxygen-Carbon monoxide
    Binary { i: 15, j: 17, beta_v: 1.0, gamma_v: 1.143174289, beta_t: 1.0, gamma_t: 0.964767932, f: 0.0, model: None }, // Oxygen-Water
    Binary { i: 15, j: 18, beta_v: 1.0, gamma_v: 1.0, beta_t: 1.0, gamma_t: 1.0, f: 0.0, model: None }, // Oxygen-Hydrogen sulfide
    Binary { i: 15, j: 19, beta_v: 1.0, gamma_v: 1.0, beta_t: 1.0, gamma_t: 1.0, f: 0.0, model: None }, // Oxygen-Helium
    Binary { i: 15, j: 20, beta_v: 0.999746847, gamma_v: 0.993907223, beta_t: 1.000023103, gamma_t: 0.990430423, f: 0.0, model: None }, // Oxygen-Argon
    Binary { i: 16, j: 17, beta_v: 1.0, gamma_v: 1.0, beta_t: 1.0, gamma_t: 1.0, f: 0.0, model: None }, // Carbon monoxide-Water
    Binary { i: 16, j: 18, beta_v: 0.795660392, gamma_v: 1.101731308, beta_t: 1.025536736, gamma_t: 1.022749748, f: 0.0, model: None }, // Carbon monoxide-Hydrogen sulfide
    Binary { i: 16, j: 19, beta_v: 1.0, gamma_v: 1.0, beta_t: 1.0, gamma_t: 1.0, f: 0.0, model: None }, // Carbon monoxide-Helium
    Binary { i: 16, j: 20, beta_v: 1.0, gamma_v: 1.159720623, beta_t: 1.0, gamma_t: 0.954215746, f: 0.0, model: None }, // Carbon monoxide-Argon
    Binary { i: 17, j: 18, beta_v: 1.0, gamma_v: 1.014832832, beta_t: 1.0, gamma_t: 0.940587083, f: 0.0, model: None }, // Water-Hydrogen sulfide
    Binary { i: 17, j: 19, beta_v: 1.0, gamma_v: 1.0, beta_t: 1.0, gamma_t: 1.0, f: 0.0, model: None }, // Water-Helium
    Binary { i: 17, j: 20, beta_v: 1.0, gamma_v: 1.038993495, beta_t: 1.0, gamma_t: 1.070941866, f: 0.0, model: None }, // Water-Argon
    Binary { i: 18, j: 19, beta_v: 1.0, gamma_v: 1.0, beta_t: 1.0, gamma_t: 1.0, f: 0.0, model: None }, // Hydrogen sulfide-Helium
    Binary { i: 18, j: 20, beta_v: 1.0, gamma_v: 1.0, beta_t: 1.0, gamma_t: 1.0, f: 0.0, model: None }, // Hydrogen sulfide-Argon
    Binary { i: 19, j: 20, beta_v: 1.0, gamma_v: 1.0, beta_t: 1.0, gamma_t: 1.0, f: 0.0, model: None }, // Helium-Argon
];
