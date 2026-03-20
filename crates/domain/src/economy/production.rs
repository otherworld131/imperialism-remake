use crate::types::*;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProductionChain {
    Timber,  // Timber → Lumber → Furniture
    Metal,   // Coal + Iron → Steel → Hardware
    Textile, // Cotton|Wool → Fabric → Clothing
}

/// The result of a production calculation.
pub struct ProductionResult {
    pub materials_produced: Vec<(MaterialType, u32)>,
    pub goods_produced: Vec<(GoodsType, u32)>,
    pub resources_consumed: Vec<(ResourceType, u32)>,
    pub materials_consumed: Vec<(MaterialType, u32)>,
    pub labor_used: u32,
}

/// Look up how much of a given resource is available in the provided slice.
fn resource_available(available: &[(ResourceType, u32)], resource: ResourceType) -> u32 {
    available
        .iter()
        .filter(|(r, _)| *r == resource)
        .map(|(_, qty)| qty)
        .sum()
}

/// Look up how much of a given material is available in the provided slice.
fn material_available(available: &[(MaterialType, u32)], material: MaterialType) -> u32 {
    available
        .iter()
        .filter(|(m, _)| *m == material)
        .map(|(_, qty)| qty)
        .sum()
}

/// Calculate production output for a mill given inputs and capacity.
///
/// Mills convert raw resources into processed materials:
/// - Timber chain: 2 timber → 1 lumber (plus 2 labor per unit)
/// - Metal chain: 1 coal + 1 iron → 1 steel (plus 2 labor per unit)
/// - Textile chain: 2 cotton/wool (can mix) → 1 fabric (plus 2 labor per unit)
///
/// Output is limited by: `min(capacity, available_resources / ratio, available_labor / 2)`
pub fn calculate_mill_production(
    chain: ProductionChain,
    available_resources: &[(ResourceType, u32)],
    mill_capacity: u32,
    available_labor: u32,
) -> ProductionResult {
    let labor_limited = available_labor / 2;

    match chain {
        ProductionChain::Timber => {
            let timber = resource_available(available_resources, ResourceType::Timber);
            let resource_limited = timber / 2;
            let units = mill_capacity.min(resource_limited).min(labor_limited);
            ProductionResult {
                materials_produced: vec![(MaterialType::Lumber, units)],
                goods_produced: vec![],
                resources_consumed: vec![(ResourceType::Timber, units * 2)],
                materials_consumed: vec![],
                labor_used: units * 2,
            }
        }
        ProductionChain::Metal => {
            let coal = resource_available(available_resources, ResourceType::Coal);
            let iron = resource_available(available_resources, ResourceType::Iron);
            let resource_limited = coal.min(iron); // 1:1 ratio
            let units = mill_capacity.min(resource_limited).min(labor_limited);
            ProductionResult {
                materials_produced: vec![(MaterialType::Steel, units)],
                goods_produced: vec![],
                resources_consumed: vec![(ResourceType::Coal, units), (ResourceType::Iron, units)],
                materials_consumed: vec![],
                labor_used: units * 2,
            }
        }
        ProductionChain::Textile => {
            let cotton = resource_available(available_resources, ResourceType::Cotton);
            let wool = resource_available(available_resources, ResourceType::Wool);
            let total_fiber = cotton + wool;
            let resource_limited = total_fiber / 2;
            let units = mill_capacity.min(resource_limited).min(labor_limited);

            // Consume resources: prefer cotton first, then wool
            let total_needed = units * 2;
            let cotton_used = cotton.min(total_needed);
            let wool_used = total_needed - cotton_used;

            let mut resources_consumed = vec![];
            if cotton_used > 0 {
                resources_consumed.push((ResourceType::Cotton, cotton_used));
            }
            if wool_used > 0 {
                resources_consumed.push((ResourceType::Wool, wool_used));
            }

            ProductionResult {
                materials_produced: vec![(MaterialType::Fabric, units)],
                goods_produced: vec![],
                resources_consumed,
                materials_consumed: vec![],
                labor_used: units * 2,
            }
        }
    }
}

/// Calculate factory production: 2 materials → 1 good (plus 2 labor per unit).
///
/// - Timber chain: 2 lumber → 1 furniture
/// - Metal chain: 2 steel → 1 hardware
/// - Textile chain: 2 fabric → 1 clothing
///
/// Output is limited by: `min(capacity, available_materials / 2, available_labor / 2)`
pub fn calculate_factory_production(
    chain: ProductionChain,
    available_materials: &[(MaterialType, u32)],
    factory_capacity: u32,
    available_labor: u32,
) -> ProductionResult {
    let labor_limited = available_labor / 2;

    let (input_material, output_good) = match chain {
        ProductionChain::Timber => (MaterialType::Lumber, GoodsType::Furniture),
        ProductionChain::Metal => (MaterialType::Steel, GoodsType::Hardware),
        ProductionChain::Textile => (MaterialType::Fabric, GoodsType::Clothing),
    };

    let available = material_available(available_materials, input_material);
    let material_limited = available / 2;
    let units = factory_capacity.min(material_limited).min(labor_limited);

    ProductionResult {
        materials_produced: vec![],
        goods_produced: vec![(output_good, units)],
        resources_consumed: vec![],
        materials_consumed: vec![(input_material, units * 2)],
        labor_used: units * 2,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // ── Timber chain — Mill ───────────────────────────────────────

    #[test]
    fn timber_mill_basic() {
        let result = calculate_mill_production(
            ProductionChain::Timber,
            &[(ResourceType::Timber, 10)],
            3,  // capacity
            20, // labor
        );
        assert_eq!(result.materials_produced, vec![(MaterialType::Lumber, 3)]);
        assert_eq!(result.resources_consumed, vec![(ResourceType::Timber, 6)]);
        assert_eq!(result.labor_used, 6);
        assert!(result.goods_produced.is_empty());
    }

    #[test]
    fn timber_mill_limited_by_resources() {
        let result = calculate_mill_production(
            ProductionChain::Timber,
            &[(ResourceType::Timber, 3)], // only enough for 1 unit (need 2 per)
            10,
            20,
        );
        assert_eq!(result.materials_produced, vec![(MaterialType::Lumber, 1)]);
        assert_eq!(result.resources_consumed, vec![(ResourceType::Timber, 2)]);
        assert_eq!(result.labor_used, 2);
    }

    #[test]
    fn timber_mill_limited_by_capacity() {
        let result = calculate_mill_production(
            ProductionChain::Timber,
            &[(ResourceType::Timber, 100)],
            2, // only capacity for 2
            20,
        );
        assert_eq!(result.materials_produced, vec![(MaterialType::Lumber, 2)]);
        assert_eq!(result.resources_consumed, vec![(ResourceType::Timber, 4)]);
        assert_eq!(result.labor_used, 4);
    }

    #[test]
    fn timber_mill_limited_by_labor() {
        let result = calculate_mill_production(
            ProductionChain::Timber,
            &[(ResourceType::Timber, 100)],
            10,
            3, // only 3 labor → 1 unit (need 2 per)
        );
        assert_eq!(result.materials_produced, vec![(MaterialType::Lumber, 1)]);
        assert_eq!(result.labor_used, 2);
    }

    #[test]
    fn timber_mill_no_resources() {
        let result = calculate_mill_production(ProductionChain::Timber, &[], 5, 20);
        assert_eq!(result.materials_produced, vec![(MaterialType::Lumber, 0)]);
        assert_eq!(result.labor_used, 0);
    }

    #[test]
    fn timber_mill_zero_capacity() {
        let result = calculate_mill_production(
            ProductionChain::Timber,
            &[(ResourceType::Timber, 10)],
            0,
            20,
        );
        assert_eq!(result.materials_produced, vec![(MaterialType::Lumber, 0)]);
        assert_eq!(result.labor_used, 0);
    }

    #[test]
    fn timber_mill_zero_labor() {
        let result =
            calculate_mill_production(ProductionChain::Timber, &[(ResourceType::Timber, 10)], 5, 0);
        assert_eq!(result.materials_produced, vec![(MaterialType::Lumber, 0)]);
        assert_eq!(result.labor_used, 0);
    }

    // ── Timber chain — Factory ────────────────────────────────────

    #[test]
    fn timber_factory_basic() {
        let result = calculate_factory_production(
            ProductionChain::Timber,
            &[(MaterialType::Lumber, 10)],
            3,
            20,
        );
        assert_eq!(result.goods_produced, vec![(GoodsType::Furniture, 3)]);
        assert_eq!(result.materials_consumed, vec![(MaterialType::Lumber, 6)]);
        assert_eq!(result.labor_used, 6);
        assert!(result.materials_produced.is_empty());
    }

    #[test]
    fn timber_factory_limited_by_materials() {
        let result = calculate_factory_production(
            ProductionChain::Timber,
            &[(MaterialType::Lumber, 5)], // enough for 2 units
            10,
            20,
        );
        assert_eq!(result.goods_produced, vec![(GoodsType::Furniture, 2)]);
        assert_eq!(result.materials_consumed, vec![(MaterialType::Lumber, 4)]);
    }

    #[test]
    fn timber_factory_limited_by_labor() {
        let result = calculate_factory_production(
            ProductionChain::Timber,
            &[(MaterialType::Lumber, 100)],
            10,
            5, // 5 labor → 2 units
        );
        assert_eq!(result.goods_produced, vec![(GoodsType::Furniture, 2)]);
        assert_eq!(result.labor_used, 4);
    }

    // ── Metal chain — Mill ────────────────────────────────────────

    #[test]
    fn metal_mill_basic() {
        let result = calculate_mill_production(
            ProductionChain::Metal,
            &[(ResourceType::Coal, 5), (ResourceType::Iron, 5)],
            3,
            20,
        );
        assert_eq!(result.materials_produced, vec![(MaterialType::Steel, 3)]);
        assert_eq!(result.labor_used, 6);
        // Should consume 3 coal and 3 iron
        assert!(result.resources_consumed.contains(&(ResourceType::Coal, 3)));
        assert!(result.resources_consumed.contains(&(ResourceType::Iron, 3)));
    }

    #[test]
    fn metal_mill_limited_by_coal() {
        let result = calculate_mill_production(
            ProductionChain::Metal,
            &[(ResourceType::Coal, 2), (ResourceType::Iron, 10)],
            5,
            20,
        );
        assert_eq!(result.materials_produced, vec![(MaterialType::Steel, 2)]);
    }

    #[test]
    fn metal_mill_limited_by_iron() {
        let result = calculate_mill_production(
            ProductionChain::Metal,
            &[(ResourceType::Coal, 10), (ResourceType::Iron, 1)],
            5,
            20,
        );
        assert_eq!(result.materials_produced, vec![(MaterialType::Steel, 1)]);
    }

    #[test]
    fn metal_mill_no_coal() {
        let result =
            calculate_mill_production(ProductionChain::Metal, &[(ResourceType::Iron, 10)], 5, 20);
        assert_eq!(result.materials_produced, vec![(MaterialType::Steel, 0)]);
    }

    #[test]
    fn metal_mill_no_iron() {
        let result =
            calculate_mill_production(ProductionChain::Metal, &[(ResourceType::Coal, 10)], 5, 20);
        assert_eq!(result.materials_produced, vec![(MaterialType::Steel, 0)]);
    }

    // ── Metal chain — Factory ─────────────────────────────────────

    #[test]
    fn metal_factory_basic() {
        let result = calculate_factory_production(
            ProductionChain::Metal,
            &[(MaterialType::Steel, 8)],
            3,
            20,
        );
        assert_eq!(result.goods_produced, vec![(GoodsType::Hardware, 3)]);
        assert_eq!(result.materials_consumed, vec![(MaterialType::Steel, 6)]);
        assert_eq!(result.labor_used, 6);
    }

    #[test]
    fn metal_factory_limited_by_steel() {
        let result = calculate_factory_production(
            ProductionChain::Metal,
            &[(MaterialType::Steel, 3)], // enough for 1
            10,
            20,
        );
        assert_eq!(result.goods_produced, vec![(GoodsType::Hardware, 1)]);
        assert_eq!(result.materials_consumed, vec![(MaterialType::Steel, 2)]);
    }

    // ── Textile chain — Mill ──────────────────────────────────────

    #[test]
    fn textile_mill_cotton_only() {
        let result = calculate_mill_production(
            ProductionChain::Textile,
            &[(ResourceType::Cotton, 6)],
            5,
            20,
        );
        assert_eq!(result.materials_produced, vec![(MaterialType::Fabric, 3)]);
        assert_eq!(result.resources_consumed, vec![(ResourceType::Cotton, 6)]);
        assert_eq!(result.labor_used, 6);
    }

    #[test]
    fn textile_mill_wool_only() {
        let result =
            calculate_mill_production(ProductionChain::Textile, &[(ResourceType::Wool, 4)], 5, 20);
        assert_eq!(result.materials_produced, vec![(MaterialType::Fabric, 2)]);
        assert_eq!(result.resources_consumed, vec![(ResourceType::Wool, 4)]);
    }

    #[test]
    fn textile_mill_mixed_cotton_and_wool() {
        let result = calculate_mill_production(
            ProductionChain::Textile,
            &[(ResourceType::Cotton, 3), (ResourceType::Wool, 3)],
            5,
            20,
        );
        // Total fiber = 6, can produce 3 fabric
        assert_eq!(result.materials_produced, vec![(MaterialType::Fabric, 3)]);
        assert_eq!(result.labor_used, 6);
        // Should consume 6 total (prefers cotton first: 3 cotton + 3 wool)
        let total_consumed: u32 = result.resources_consumed.iter().map(|(_, q)| q).sum();
        assert_eq!(total_consumed, 6);
    }

    #[test]
    fn textile_mill_mixed_partial() {
        // 1 cotton + 1 wool = 2 fiber → 1 fabric
        let result = calculate_mill_production(
            ProductionChain::Textile,
            &[(ResourceType::Cotton, 1), (ResourceType::Wool, 1)],
            5,
            20,
        );
        assert_eq!(result.materials_produced, vec![(MaterialType::Fabric, 1)]);
        let total_consumed: u32 = result.resources_consumed.iter().map(|(_, q)| q).sum();
        assert_eq!(total_consumed, 2);
    }

    #[test]
    fn textile_mill_no_fiber() {
        let result = calculate_mill_production(ProductionChain::Textile, &[], 5, 20);
        assert_eq!(result.materials_produced, vec![(MaterialType::Fabric, 0)]);
        assert_eq!(result.labor_used, 0);
    }

    // ── Textile chain — Factory ───────────────────────────────────

    #[test]
    fn textile_factory_basic() {
        let result = calculate_factory_production(
            ProductionChain::Textile,
            &[(MaterialType::Fabric, 10)],
            4,
            20,
        );
        assert_eq!(result.goods_produced, vec![(GoodsType::Clothing, 4)]);
        assert_eq!(result.materials_consumed, vec![(MaterialType::Fabric, 8)]);
        assert_eq!(result.labor_used, 8);
    }

    #[test]
    fn textile_factory_limited_by_fabric() {
        let result = calculate_factory_production(
            ProductionChain::Textile,
            &[(MaterialType::Fabric, 1)], // not enough for 1 unit
            5,
            20,
        );
        assert_eq!(result.goods_produced, vec![(GoodsType::Clothing, 0)]);
        assert_eq!(result.labor_used, 0);
    }

    // ── Edge cases ────────────────────────────────────────────────

    #[test]
    fn factory_zero_capacity() {
        let result = calculate_factory_production(
            ProductionChain::Metal,
            &[(MaterialType::Steel, 100)],
            0,
            20,
        );
        assert_eq!(result.goods_produced, vec![(GoodsType::Hardware, 0)]);
        assert_eq!(result.labor_used, 0);
    }

    #[test]
    fn factory_zero_labor() {
        let result = calculate_factory_production(
            ProductionChain::Timber,
            &[(MaterialType::Lumber, 100)],
            10,
            0,
        );
        assert_eq!(result.goods_produced, vec![(GoodsType::Furniture, 0)]);
        assert_eq!(result.labor_used, 0);
    }

    #[test]
    fn all_chains_mill_and_factory() {
        // Ensure all three chains work end-to-end
        for chain in [
            ProductionChain::Timber,
            ProductionChain::Metal,
            ProductionChain::Textile,
        ] {
            let resources: Vec<(ResourceType, u32)> = match chain {
                ProductionChain::Timber => vec![(ResourceType::Timber, 20)],
                ProductionChain::Metal => {
                    vec![(ResourceType::Coal, 10), (ResourceType::Iron, 10)]
                }
                ProductionChain::Textile => vec![(ResourceType::Cotton, 20)],
            };

            let mill_result = calculate_mill_production(chain, &resources, 5, 20);
            assert!(
                mill_result.materials_produced.iter().any(|(_, q)| *q > 0),
                "Mill should produce materials for {chain:?}"
            );

            let factory_result =
                calculate_factory_production(chain, &mill_result.materials_produced, 5, 20);
            // Factory needs 2 materials per good, so with 5 materials max → 2 goods
            // Factory should produce some result (possibly 0 if not enough
            // materials, but the vector itself should not be empty).
            assert!(
                !factory_result.goods_produced.is_empty(),
                "Factory should handle production for {chain:?}"
            );
        }
    }

    #[test]
    fn mill_result_consumed_matches_produced_ratio() {
        // Timber: 2 timber per 1 lumber
        let result = calculate_mill_production(
            ProductionChain::Timber,
            &[(ResourceType::Timber, 20)],
            5,
            20,
        );
        let produced = result.materials_produced[0].1;
        let consumed = result.resources_consumed[0].1;
        assert_eq!(consumed, produced * 2);
    }

    #[test]
    fn factory_result_consumed_matches_produced_ratio() {
        // 2 materials per 1 good
        let result = calculate_factory_production(
            ProductionChain::Timber,
            &[(MaterialType::Lumber, 20)],
            5,
            20,
        );
        let produced = result.goods_produced[0].1;
        let consumed = result.materials_consumed[0].1;
        assert_eq!(consumed, produced * 2);
    }
}
