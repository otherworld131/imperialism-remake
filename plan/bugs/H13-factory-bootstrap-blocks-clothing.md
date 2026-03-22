# H13. Factory bootstrap blocks ClothingFactory — prevents immigration

**Severity:** High — FIXED

**Root cause:** Factory building required 1 Lumber + 1 Steel per factory. After building
FurnitureFactory and HardwareFactory (consuming 2 Steel total), insufficient Steel remained
for ClothingFactory. Without Clothing production, immigration never triggers (requires
CannedFood + Clothing + Furniture simultaneously).

**Fix:**
- [x] First factory of each type is now free (same bootstrap as mills) for both
  human auto-play and AI
- Human now has all 3 factories, produces Clothing, and has Transport score > 0
