This style is a "crisp pixel accent" look built for a dense colony sim. It is not trying to look like old hardware and it is not trying to look painterly. The goal is modern readability first, with a restrained pixel structure that gives character without ever turning into blur, smear, or noisy mush.

Overall read and priorities  
At the most common zoom level, the player must instantly parse: walkable ground vs blocked, room boundaries, doors, interactable machines, items, pawns, and threats. The art must support gameplay clarity over decoration. At closer zoom, the player should notice pleasing pixel clusters and material motifs. If you have to zoom in to understand what something is, the sprite is wrong.

The signature look in one sentence  
Clean silhouettes and banded shading, with interior detail expressed as deliberate pixel clusters, rendered with crisp sampling and zero blur.

Camera and perspective language  
Use a consistent top-down or very slight "tilt" perspective. Do not mix viewpoints between assets. Objects should feel like they live in the same projection: if walls show a little thickness, everything that has height should show it the same way. Avoid strong isometric angles. The world should read like a plan view with just enough depth cues to feel physical, not like a 3D render.

Pixel treatment and rendering assumptions  
World art is designed to be displayed with nearest-neighbor sampling. That means every pixel edge is sacred. Never rely on anti-aliasing or blur to "fix" edges. If an edge looks jagged in a bad way, redesign the contour or adjust the pixel clusters. Do not add blur to low resolution textures. Do not paint with soft airbrush gradients that depend on filtering to look smooth.

The "pixel accent" idea means you are not making chunky 8-bit sprites. You are making clean, modern shapes, then simplifying and quantizing the interior detail into visible pixel clusters. Think "controlled blockiness" inside a clean form.

Line and edge rules  
Pick one edge policy and apply it consistently across the entire game.

Preferred policy for colony sim readability: "interactable outline."  
Interactable objects and pawns get a thin, consistent outline. Terrain does not. This makes dense scenes readable without noisy textures.

Outline specifics:  
Use a single outline color family, typically a dark neutral, not pure black. Keep outline thickness consistent across assets at the same scale. Do not vary outline thickness to imply importance. Corners can be slightly stepped. Avoid sub-pixel curves. If you need a curve, do it with clean stair-steps and keep the contour simple.

If you do not use outlines, then you must enforce the alternative policy: every object must have strong value contrast against whatever terrain it commonly sits on. That usually forces you into darker ground and lighter objects or vice versa. Most colony sims benefit from the outline policy because it reduces palette conflicts.

Shading language and value discipline  
This style uses banded shading. You do not paint smooth gradients. You choose a small number of value steps and commit to them.

Typical per-material shading:  
2 to 4 value levels for the main body. One highlight band. One shadow band. Sometimes a tiny accent specular on metal.

Shadows are shaped, not blurry:  
Cast shadows and occlusion are simple, crisp shapes with limited softness. If you need softness, you do it with 1 step of value change, not blur.

Value separation is the main readability tool:  
Two different materials must differ in lightness clearly, even if their hues are close. If wood and dirt share similar lightness, the scene becomes muddy. Values are more important than hue.

Color palette rules  
The palette is constrained but not crushed. You can have a limited set of hues, but you must have enough value steps to keep materials distinct.

General palette guidance:  
Use moderate saturation for most surfaces. Save high saturation for signals: selection states, warnings, faction color accents, special resources. Neutrals are clean, not brown-muddy. Avoid the "everything is beige-gray" look.

Avoid dithering as default:  
Dithering tends to create vibration and noise in a crisp-sampled game. Only use dithering as a rare, intentional effect for something like fog, a magical field, or a special biome texture. Even then, keep it subtle and low contrast.

Texture and detail rules  
This is where the style becomes unique and where many artists accidentally break it.

No micro-noise. No speckle. No "photographic" texture.  
If you add random noise to suggest texture, it will shimmer and look dirty when the camera moves or when zoom changes. Texture must be organized into clusters and motifs.

Texture must be low-frequency and patterned:  
Think in blocks: larger, readable clusters that imply grain, stone blocks, metal panels, cloth weave. If you cannot clearly see the texture pattern at the common zoom, it is too fine and must be simplified.

Interior detail is allowed, but it is subordinate:  
The silhouette and the big shading bands always win. Interior details must never break the outer read. If interior detail starts to compete with the silhouette, delete it.

Material motif library (shared visual vocabulary)  
To keep multiple artists consistent, you want a small "motif library" that everyone uses. These motifs should be repeated across assets so the world feels cohesive.

Examples of motifs, described conceptually:  
Wood: broad grain bands or plank segmentation, not scribbly grain.  
Stone: block segmentation with a few chips, not noisy rock texture.  
Metal: panel lines, bolts, a clean highlight band, occasional scuff clusters.  
Concrete: smooth with a couple of cracks or seams, not speckle.  
Fabric: simple folds with banded shading, no fine weave noise.  
Soil/sand: gentle clusters and occasional stones, but stones are grouped, not sprinkled.  
Water: large repeating wave clusters or calm gradient bands, no tiny checker noise.

Terrain rules (critical for colony sims)  
Terrain must be calm. Terrain is the "background layer" that supports readability. It cannot compete with buildings or items.

Terrain texture frequency must be lower than object texture frequency:  
Terrain should be large clusters and gentle patterns. Objects can have slightly more detail, but still controlled. The moment your terrain has high contrast micro detail, your world becomes busy and tiring.

Paths and floors must be extremely readable:  
If you have floors and paths, they should have clear edging or pattern differences that are visible at distance. Use value shifts and organized motifs, not noise.

Object design rules (buildings, machines, furniture)  
Objects should be icon-clear. Imagine each object as a pictogram first, then decorate it.

Silhouette first:  
Every interactable has a primary shape that is recognizable. Avoid overly complex outlines. "Big shape plus one secondary protrusion" reads better than "many little nubs."

Use "feature grouping":  
Details come in grouped clusters: a control panel cluster, a pipe cluster, a vent cluster. Avoid scattering tiny details all over the sprite.

Use consistent exaggerations:  
Pick what the world exaggerates. Maybe all machines have slightly oversized bolts and vents. Maybe all doors have a strong frame. Pick 2 or 3 exaggerations and reuse them everywhere.

Character style rules (your strongest uniqueness lever)  
Characters are where you can stop looking like RimWorld, Stardew, or DF instantly. Lock proportions and facial detail policy.

Proportion policy:  
Choose a silhouette that is not a RimWorld pawn and not a Stardew chibi. For example: slightly taller body, more distinct shoulders, less circular head, or a unique helmet/hair silhouette language.

Detail policy:  
Faces can be minimal, but consistent. If you show eyes, always show eyes the same way. If you omit faces, lean on hair/helmet and outfit silhouettes for identity. Avoid soft facial shading.

Outfits and gear:  
Gear should read with bold shapes and small, consistent accents. Avoid cluttering the pawn with lots of tiny items. If you want equipment readability, use clear slot silhouettes: a backpack shape, a shoulder pad shape, a weapon silhouette.

Item and icon rules  
Items should look like clean icons dropped into the world. They must read instantly at small size.

Use a limited icon shading model:  
One highlight band, one shadow band. Minimal internal lines. Strong silhouette. If you have stacks, make stacks readable with simple repetition, not messy piling.

State and feedback visuals  
This style benefits from clean "state layers" instead of baking states into sprites.

Use overlay accents for states:  
Damage, selection, forbidden, owned, powered, etc. should primarily be communicated by crisp overlays, outlines, badges, and small icons. Do not repaint base sprites into dozens of variants early.

Animation and motion look  
Movement can be smooth, but sprite placement must look stable. Avoid sub-pixel crawling. If the camera moves, sprites should remain crisp and not shimmer. Particles should be chunky and consistent, not blurry. Smoke and dust can be stylized with cluster shapes and limited alpha steps, not soft blur.

Common failure modes to avoid (what makes it look like "mud")

1. Airbrushed gradients that depend on filtering.  
2. Tiny noisy textures on terrain or large surfaces.  
3. Dithering everywhere.  
4. Inconsistent pixel grid between assets.  
5. Different artists using different outline thicknesses or different shadow softness.  
6. Too many similar midtone values, causing materials to blend.  
7. Making UI pixelated and losing clarity.

Artist workflow guidance for consistency  
To keep multiple artists aligned, enforce these practices:

Start every sprite as a silhouette thumbnail. Confirm readability at the target size before adding detail.  
Apply banded shading using a limited ramp. Do not blend.  
Add texture only as organized clusters that follow the material motif library.  
Test the sprite on top of your common terrain tiles. If it blends, fix values or outline policy.  
Check the sprite at the most common zoom. If the sprite relies on tiny details, simplify.  
Keep a shared reference sheet: palette ramps, outline color, shadow color, and example sprites.

Deliverable standards (what an artist hands off)  
For each asset, the artist should deliver:  
A base sprite that reads cleanly at target size with crisp edges.  
A palette ramp used for that asset's primary materials.  
Confirmation of which motif rules were used (wood motif, metal motif, stone motif).  
A quick "zoom test" screenshot on typical terrain with neighboring objects.


