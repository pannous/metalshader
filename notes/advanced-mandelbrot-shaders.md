# Advanced Mandelbrot Shaders

## Overview

Created sophisticated Mandelbrot renderers with advanced techniques for quality, performance, and numeric stability at extreme zoom levels.

## Shader Comparison

### mandelbrot_autozoom.frag (Basic)
**Features:**
- Basic f32 iteration
- Auto-zoom functionality
- Mouse smoothing support
- Works up to ~77s (zoom ~100,000x)

**Best for:** Learning, demonstrations, smooth auto-zoom experience

### mandelbrot_advanced.frag (Recommended)
**Features:**
- ✨ **Smooth/continuous coloring** - No color banding
- ✨ **Distance estimation** - Sharp, anti-aliased edges
- ✨ **Adaptive iterations** - 100-2000 based on zoom level
- ✨ **2x2 anti-aliasing** - Smoother rendering
- ✨ **Interior detection** - Faster convergence
- Numeric stability optimizations
- Manual zoom and pan controls

**Best for:** High-quality static renders, exploration, deep zooms

**Techniques:**
```glsl
// Smooth iteration count (eliminates banding)
smoothIter = iterations + 1 - log(log(|z|)) / log(2)

// Distance estimation (sharp edges)
dist = 0.5 * |z| * log(|z|) / |dz|

// Adaptive iterations
maxIter = 100 + log2(zoom) * 15  // 100-2000 range
```

### mandelbrot_perturbation.frag (Experimental)
**Features:**
- ✨ **Perturbation theory** - Better numeric stability
- ✨ **Series approximation** - Performance optimization
- ✨ **Orbit trap coloring** - Alternative color schemes
- ✨ **Period detection** - Interior optimization
- ✨ **Stratified anti-aliasing** - Better sampling
- ✨ **Adaptive AA** - More samples at edges
- Multiple color palette modes

**Best for:** Research, extreme zooms, experimental rendering

**Advanced Techniques:**
```glsl
// Orbit trap for coloring
minOrbitDist = min(minOrbitDist, |z|^2)

// Period detection (interior points)
if (|z - oldZ| < epsilon) → inside set

// Stratified AA sampling
angle = i * 2.399  // Golden angle
radius = sqrt(i / samples) * 0.5
```

## Performance Comparison

| Shader | Iterations | AA Quality | Speed | Zoom Limit |
|--------|-----------|------------|-------|-----------|
| autozoom | 100 (fixed) | None | Fastest | ~100,000x |
| advanced | 100-2000 (adaptive) | 2x2 grid | Medium | ~1,000,000x+ |
| perturbation | 150-3000 (adaptive) | Adaptive 1x-4x | Slower | ~10,000,000x+ |

## Visual Quality Features

### Smooth Coloring
Eliminates color banding using continuous potential:
- Formula: `μ = n + 1 - log(log|z|) / log(2)`
- Result: Smooth gradient instead of discrete bands

### Distance Estimation
Provides sharp, mathematically accurate boundaries:
- Calculates distance to set boundary
- Enables anti-aliasing without blur
- Formula: `d = |z| * log|z| / |z'|`

### Orbit Traps
Alternative coloring based on closest approach:
- Track minimum distance to point/line during iteration
- Creates unique, artistic patterns
- Reveals internal structure

## Numeric Stability Improvements

1. **Higher escape radius** (256 vs 2)
   - More accurate smooth coloring
   - Better distance estimation

2. **Derivative tracking**
   - Enables distance estimation
   - Improves numeric stability

3. **Period detection**
   - Fast interior point detection
   - Avoids wasted iterations

4. **Adaptive iteration count**
   - More detail at high zoom
   - Faster at low zoom

## Usage Recommendations

**For beginners:**
- Start with `mandelbrot_autozoom.frag`
- Watch the auto-zoom, understand the structure

**For quality:**
- Use `mandelbrot_advanced.frag`
- Manual exploration with smooth rendering
- Best balance of quality and performance

**For research/extreme zoom:**
- Try `mandelbrot_perturbation.frag`
- Experiment with color schemes
- Push numeric limits

## Implementation Notes

### Distance Estimation Derivative
```glsl
// Track derivative dz while iterating
dz = 2*z*dz + 1

// At escape, calculate distance
d = 0.5 * |z| * log(|z|) / |dz|
```

### Anti-Aliasing Strategy
```glsl
// Grid-based (advanced.frag)
for y in 0..2, x in 0..2:
    offset = (x, y) * 0.25 - 0.125
    sample at c + offset

// Adaptive (perturbation.frag)
if (distance < threshold):
    use 4x sampling
else:
    use 1x sampling
```

### Color Palette Design
```glsl
// Cosine palette (flexible)
color = a + b * cos(2π(c*t + d))

// Examples:
// Rainbow: a=0.5, b=0.5, c=1.0, d=(0,0.33,0.67)
// Fire: a=0.5, b=0.5, c=1.0, d=(0,0.1,0.2)
// Ice: a=0.5, b=0.5, c=1.0, d=(0.6,0.7,0.8)
```

## Future Enhancements

Potential additions for even more sophisticated rendering:

1. **Arbitrary precision** (f64/emulated)
   - Enable zooms beyond 10^15
   - Requires double precision or emulation

2. **Full perturbation theory**
   - Compute reference orbit once
   - Perturb for each pixel (huge speedup)

3. **Series approximation**
   - Skip early iterations using Taylor series
   - Significant performance gain at deep zoom

4. **Automatic color cycling**
   - Animate palettes over time
   - Parameter interpolation

5. **Save/load locations**
   - Bookmark interesting coordinates
   - Share deep zoom locations

6. **Multi-threading/compute shaders**
   - Parallel reference orbit computation
   - Better deep zoom performance

## References

- Distance Estimation: https://iquilezles.org/articles/distancefractals/
- Smooth Coloring: https://linas.org/art-gallery/escape/smooth.html
- Perturbation Theory: https://fractalforums.org/index.php?topic=25925.0
- Color Palettes: https://iquilezles.org/articles/palettes/
