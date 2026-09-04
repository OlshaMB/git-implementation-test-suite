from .branching_history import generate as branching_history
from .competing_bases import generate as competing_bases
from .delta_boundaries import generate as delta_boundaries
from .depth_pressure import generate as depth_pressure
from .linear_text_history import generate as linear_text_history
from .tiny_mixed import generate as tiny_mixed

RECIPES = {
    "branching-history": branching_history,
    "competing-bases": competing_bases,
    "delta-boundaries": delta_boundaries,
    "depth-pressure": depth_pressure,
    "linear-text-history": linear_text_history,
    "tiny-mixed": tiny_mixed,
}
