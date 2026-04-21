# Voxel Tree Generation Algorithms Research

Research conducted 2026-01-10 for improving old-growth oak tree generation.

## Table of Contents
1. [Current Implementation Analysis](#current-implementation-analysis)
2. [Minecraft Big Oak Algorithm](#minecraft-big-oak-algorithm)
3. [Space Colonization Algorithm](#space-colonization-algorithm)
4. [Self-Organizing Tree Generation](#self-organizing-tree-generation)
5. [Old-Growth Oak Botanical Characteristics](#old-growth-oak-botanical-characteristics)
6. [Recommended Hybrid Approach](#recommended-hybrid-approach)

---

## Current Implementation Analysis

**File**: `src/world_gen/trees/oak.rs`

### Normal Oak (90% of oaks)
- Height: 4-9 blocks
- Simple trunk + spheroid canopy
- Optional 1-2 branches for tall, large variants
- 3-5 leaf layers

### Giant Oak (10% of oaks) - PROBLEMATIC
Uses a "multi-deck" system that produces artificial-looking results:
- 2-3 separate canopy levels stacked vertically
- Cross-bracing between vertical supports
- Horizontal branches with occasional vertical "risers"

**Problem**: Looks like platforms on a pole, not organic old-growth oak.

---

## Minecraft Big Oak Algorithm

**Source**: [Earthcomputer's Gist](https://gist.github.com/Earthcomputer/41addf80c12d001dfa4391c3a0d03be8)

### Key Parameters
```
heightLimit: 5-16 blocks (random)
trunkHeight: heightLimit × 0.618 (golden ratio)
leafDistanceLimit: 4 (sapling) or 5 (natural growth)
branchSlope: 0.381 gradient ratio
branchCount: 2 if heightLimit >= 11, otherwise 1 per y-layer
```

### Generation Process

1. **Validation**: Sapling location must be dirt/grass/farmland with clear space up to heightLimit. Algorithm decreases heightLimit if obstructed (minimum 6 blocks).

2. **Branch Placement**:
   - For each y-layer from `heightLimit - leafDistanceLimit` to `heightLimit × 0.3`
   - Generate `branchCount` branches ending at that y-level
   - Maximum horizontal distance: all branches fit inside sphere of radius `heightLimit × 0.664`
   - Actual distance randomized: minimum 0.247× maximum length
   - Branch connects to trunk at lower y-position (respecting branchSlope)

3. **Branch Log Generation**:
   - Only if connection point > 20% of heightLimit
   - Straight line from trunk connection to endpoint
   - One log per x-coordinate (if further in x) or z-coordinate

4. **Leaf Generation**:
   - Sphere of leaves with radius = leafDistanceLimit around each branch endpoint

5. **Trunk**: `trunkHeight + 1` log blocks vertically

### Pros
- Fast and deterministic
- Proven in Minecraft
- Simple to implement

### Cons
- Branches are straight lines
- Limited organic feel

---

## Space Colonization Algorithm

**Sources**:
- [ProceduralVoxelTree GitHub](https://github.com/joesobo/ProceduralVoxelTree)
- [Modeling Trees with Space Colonization](https://www.researchgate.net/publication/221314843_Modeling_Trees_with_a_Space_Colonization_Algorithm)

### Core Concept
Bio-inspired approach where branches grow toward unoccupied space points.

### Algorithm Steps

1. **Define Crown Shape**: Distribute "attractor points" in desired crown volume (sphere, ellipsoid, custom shape)

2. **Initialize**: Place root/trunk base

3. **Iteration Loop**:
   ```
   For each bud (growing tip):
     1. Find attractor points within perception cone
     2. Calculate optimal growth direction (average of vectors to attractors)
     3. Grow new branch segment in that direction
     4. Mark nearby attractor points as "occupied"
   ```

4. **Termination**: Stop when no more attractors or maximum iterations

### Key Parameters
- **Perception Volume**: Cone from bud defining visible attractors
- **Kill Distance**: Distance at which attractors are removed
- **Branch Segment Length**: Length of each growth step
- **Tropism Vectors**: Gravity/light influence on growth direction

### Pros
- Most realistic branching patterns
- Natural space-filling behavior
- Breaks L-system symmetry

### Cons
- Computationally expensive
- Harder to make deterministic
- More complex implementation

---

## Self-Organizing Tree Generation

**Source**: [Caner's Blog - Procedural Tree Generation (2024)](https://caner-milko.github.io/posts/procedural-tree-generation/)

### Resource Competition Model
Trees modeled as dynamic systems where branches compete for resources.

### Growth Iteration
```
1. Calculate light/resource at each bud (ray casting or voxel shadow map)
2. Accumulate resources recursively toward root
3. Distribute vigor to child branches using apical control (λ parameter)
4. Buds with sufficient vigor become new branches
5. Prune (shed) underperforming buds
```

### Apical Control Parameter (λ)
- **Low λ (0.46)**: Tree grows more laterally (spreading crown)
- **High λ (0.54)**: Tree grows more vertically (upright form)
- **λ = 0.50**: Balanced growth

### Key Insight for Oaks
Oaks have low apical control - they develop wide, spreading crowns rather than dominant central leaders.

---

## Old-Growth Oak Botanical Characteristics

**Sources**:
- [Clemson Live Oak Factsheet](https://hgic.clemson.edu/factsheet/live-oak/)
- [Piedmont Master Gardeners - White Oak](https://piedmontmastergardeners.org/article/white-oak-a-majestic-native-species/)
- [National Wildlife Federation - Southern Live Oak](https://www.nwf.org/Educational-Resources/Wildlife-Guide/Plants-and-Fungi/Southern-Live-Oak)

### Trunk Characteristics
- Massive: Can grow to 6+ feet diameter
- Squat, tapering form
- Often larger diameter than height would suggest
- May fork into multiple major limbs partway up

### Branch Structure (Key for Realism)
- **Decurrent Growth Pattern**: Broad spreading canopy, NOT dominant central trunk
- **Sweeping Limbs**: Branches "plunge toward ground then shoot upward"
- **Curvy, Not Angular**: No sharp angles, gradual curves throughout
- **Radial Arrangement**: Branches can have radial alternate arrangement, not single plane
- **Multiple Strong Branches**: All unusually curvy

Quote from research:
> "Live oaks exhibit a decurrent growth pattern, meaning they develop a broad, spreading canopy rather than a central dominant trunk."

### Canopy Shape
- Width often equals height (100-150 feet spread possible)
- Crown can reach 150 feet diameter
- Multiple canopy layers but integrated, not stacked platforms
- Open-grown trees are "massive, picturesque, wide-spreading"

### Longevity
- Live oaks: 200-300 years typical, some 600+ years
- White oaks: "200 years growing, 200 years living, 200 years dying"

---

## Recommended Hybrid Approach

Combine Minecraft's simplicity with key insights from space colonization and real oak structure.

### Algorithm: "Majestic Oak"

#### Key Features
1. **Forking Trunk** (50% variant): Main trunk splits into 2-4 major limbs at 40-60% height
2. **Single Trunk** (50% variant): Continuous trunk with crown of radiating branches
3. **Curved Branches**: 2-3 segment paths with gradual direction changes
4. **Droop-Rise Pattern**: Branches start horizontal/downward, then curve upward
5. **Sphere-Bounded Distribution**: Endpoints within `height × 0.7` radius sphere

#### Parameters
```
height: 12-24 blocks
trunk_fork_height: height × 0.4 to 0.6
num_major_limbs: 2-4 (forking variant)
branch_endpoints: 8-16 per tree
leaf_cluster_radius: 3-5 blocks
```

#### Branch Curve Implementation
```
For each branch (attachment point to endpoint):
  1. Calculate 2-3 control points
  2. First segment: -10° to -30° (droop)
  3. Middle: horizontal transition
  4. Final segment: +20° to +45° (rise)
  5. Linear interpolation between points
```

This avoids true Bezier math while achieving organic curves.

---

## References

1. Earthcomputer. "A description of the big oak tree growth algorithm in Minecraft." GitHub Gist. https://gist.github.com/Earthcomputer/41addf80c12d001dfa4391c3a0d03be8

2. Sobo, Joe. "ProceduralVoxelTree." GitHub. https://github.com/joesobo/ProceduralVoxelTree

3. Runions et al. "Modeling Trees with a Space Colonization Algorithm." ResearchGate, 2007. https://www.researchgate.net/publication/221314843

4. Milko, Caner. "Procedural Tree Generation - TreeGen Part 1." September 2024. https://caner-milko.github.io/posts/procedural-tree-generation/

5. Clemson University. "Live Oak Factsheet." https://hgic.clemson.edu/factsheet/live-oak/

6. Piedmont Master Gardeners. "White Oak - A Majestic Native Species." https://piedmontmastergardeners.org/article/white-oak-a-majestic-native-species/

7. National Wildlife Federation. "Southern Live Oak." https://www.nwf.org/Educational-Resources/Wildlife-Guide/Plants-and-Fungi/Southern-Live-Oak

8. Minecraft Wiki. "Tree" and "Oak." https://minecraft.wiki/w/Tree
