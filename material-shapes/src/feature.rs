use crate::cubic::{Cubic, PointTransformer};
use crate::utils::{require, DISTANCE_EPSILON};

pub struct FeatureFactory;
impl FeatureFactory {
    /// Group a list of [`Cubic`] objects into a feature that should be ignored in the default
    /// [`Morph`](crate::morph::Morph) mapping. The feature can have any indentation.
    ///
    /// Sometimes it's helpful to ignore certain features when morphing shapes. This is because
    /// only the features you mark as important will be smoothly transitioned between the start
    /// and end shapes. Additionally, the default morph algorithm will try to match convex
    /// corners to convex corners and concave to concave. Marking features as ignorable will
    /// influence this matching. For example, given a 12-pointed star, marking all concave
    /// corners as ignorable will create a [`Morph`](crate::morph::Morph) that only considers the outer corners of the
    /// star. As a result, depending on the morphed-to shape, the animation may have fewer
    /// intersections and rotations. Another example for the other way around is a [`Morph`](crate::morph::Morph)
    /// between a pointed-up triangle to a square. Marking the square's top edge as a convex
    /// corner matches it to the triangle's upper corner. Instead of moving the triangle's upper
    /// corner to one of the rectangle's corners, the animation now splits the triangle to match
    /// the square's outer corners.
    ///
    /// `cubics` is the list of raw cubics describing the feature's shape.
    ///
    /// # Panics
    ///
    /// Panics if the list of cubics is empty or contains non-continuous cubics.
    pub fn build_ignorable_feature(cubics: Vec<Cubic>) -> Feature {
        Self::validate(Feature::edge(cubics))
    }


    /// Groups a [`Cubic`] object to an edge (neither inward nor outward identification in a shape).
    ///
    /// `cubic` is the raw cubic describing the edge's shape.
    ///
    /// # Panics
    ///
    /// Panics if the cubic is empty or non-continuous.
    pub fn build_edge(cubic: Cubic) -> Feature {
        Feature::edge(vec![cubic])
    }

    /// Groups a list of [`Cubic`] objects into a convex corner (outward indentation in a shape).
    ///
    /// `cubics` is the list of raw cubics describing the corner's shape.
    ///
    /// # Panics
    ///
    /// Panics if the list of cubics is empty or contains non-continuous cubics.
    pub fn build_convex_corner(cubics: Vec<Cubic>) -> Feature {
        Self::validate(Feature::corner(cubics, true))
    }
    /// Groups a list of [`Cubic`] objects into a concave corner (inward indentation in a shape).
    ///
    /// `cubics` is the list of raw cubics describing the corner's shape.
    ///
    /// # Panics
    ///
    /// Panics if the list of cubics is empty or contains non-continuous cubics.
    pub fn build_concave_corner(cubics: Vec<Cubic>) -> Feature {
        Self::validate(Feature::corner(cubics, false))
    }

    fn validate(feature: Feature) -> Feature {
        require(
            !feature.cubics().is_empty(),
            "Features need at least one cubic."
        );
        require(
            Self::is_continuous(&feature),
            "Feature must be continuous, with the anchor points of all cubics matching the anchor points of the preceding and succeeding cubics"
        );
        feature
    }
    fn is_continuous(feature: &Feature) -> bool {
        let mut prev_cubic = &feature.cubics()[0];
        for index in 1..feature.cubics().len() {
            let cubic = &feature.cubics()[index];
            if (cubic.anchor_0_x() - prev_cubic.anchor_1_x()).abs() > DISTANCE_EPSILON ||
                (cubic.anchor_0_y() - prev_cubic.anchor_1_y()).abs() > DISTANCE_EPSILON {
                return false;
            }
            prev_cubic = cubic;
        }
        true
    }
}

/// While a polygon's shape can be drawn solely using a list of [`Cubic`] objects representing its raw
/// curves and lines, features add an extra layer of context to groups of cubics. Features group
/// cubics into (straight) edges, convex corners, or concave corners. For example, rounding a
/// rectangle adds many cubics around its edges, but the rectangle's overall number of corners
/// remains the same. [`Morph`](crate::morph::Morph) therefore uses this grouping for several reasons:
///
/// - **Noise Reduction**: Grouping cubics reduces the amount of noise introduced by individual cubics
///   (as seen in the rounded rectangle example).
/// - **Mapping Base**: The grouping serves as the base set for [`Morph`](crate::morph::Morph)’s mapping process.
/// - **Curve Type Mapping**: [`Morph`](crate::morph::Morph) maps similar curve types (convex, concave) together. Note that
///   edges or features created with [`build_ignorable_feature`](FeatureFactory::build_ignorable_feature) are ignored in the default mapping.
///
/// By using features, you can manipulate polygon shapes with more context and control.

#[derive(Clone, Debug, Hash, Eq, PartialEq)]
pub enum Feature {
    /// Edges have only a list of the cubic curves which make up the edge. Edges lie between corners
    /// and have no vertex or concavity; the curves are simply straight lines (represented by Cubic
    /// curves).
    Edge(Edge),
    /// Corners contain the list of cubic curves which describe how the corner is rounded (or not),
    /// and a flag indicating whether the corner is convex. A regular polygon has all convex corners,
    /// while a star polygon generally (but not necessarily) has both convex (outer) and concave
    /// (inner) corners.
    Corner(Corner),
}

impl Feature {
    pub fn edge(cubics: Vec<Cubic>) -> Self {
        Feature::Edge(Edge::new(cubics))
    }
    pub fn corner(cubics: Vec<Cubic>, convex: impl Into<Option<bool>>) -> Self {
        Feature::Corner(Corner::new(cubics, convex))
    }
}
impl FeatureTrait for Feature {
    fn cubics(&self) -> &Vec<Cubic> {
        match self {
            Feature::Edge(edge) => edge.cubics(),
            Feature::Corner(corner) => corner.cubics(),
        }
    }

    /// Transforms the points in this [`Feature`] with the given [`PointTransformer`] and returns a new
    /// [`Feature`].
    ///
    /// `f` is the [`PointTransformer`] used to transform this [`Feature`].
    fn transformed(&self, f: &PointTransformer) -> Feature {
        match self {
            Feature::Edge(edge) => edge.transformed(f),
            Feature::Corner(corner) => corner.transformed(f),
        }
    }

    /// Returns a new [`Feature`] with the points that define the shape of this [`Feature`] in reversed
    /// order.
    fn reversed(&self) -> Feature {
        match self {
            Feature::Edge(edge) => edge.reversed(),
            Feature::Corner(corner) => corner.reversed(),
        }
    }

    /// Whether this Feature gets ignored in the Morph mapping. See [`build_ignorable_feature`](FeatureFactory::build_ignorable_feature) for more
    /// details.
    fn is_ignorable_feature(&self) -> bool {
        match self {
            Feature::Edge(edge) => edge.is_ignorable_feature(),
            Feature::Corner(corner) => corner.is_ignorable_feature(),
        }
    }

    /// Whether this Feature is an Edge with no inward or outward indentation.
    fn is_edge(&self) -> bool {
        match self {
            Feature::Edge(edge) => edge.is_edge(),
            Feature::Corner(corner) => corner.is_edge(),
        }
    }

    /// Whether this Feature is an Edge with no inward or outward indentation.
    fn is_convex_corner(&self) -> bool {
        match self {
            Feature::Edge(edge) => edge.is_convex_corner(),
            Feature::Corner(corner) => corner.is_convex_corner(),
        }
    }

    /// Whether this Feature is an Edge with no inward or outward indentation.
    fn is_concave_corner(&self) -> bool {
        match self {
            Feature::Edge(edge) => edge.is_concave_corner(),
            Feature::Corner(corner) => corner.is_concave_corner(),
        }
    }
}

pub trait FeatureTrait {
    fn cubics(&self) -> &Vec<Cubic>;
    /**
    * Transforms the points in this [Feature] with the given [PointTransformer] and returns a new
    * [Feature]
    *
    * @param f The [PointTransformer] used to transform this [Feature]
    */
    fn transformed(&self, f: &PointTransformer) -> Feature;
    /**
    * Returns a new [Feature] with the points that define the shape of this [Feature] in reversed
    * order.
    */
    fn reversed(&self) -> Feature;
    /**
    * Whether this Feature gets ignored in the Morph mapping. See [build_ignorableFeature](FeatureFactory::build_ignorable_feature) for more
    * details.
    */
    fn is_ignorable_feature(&self) -> bool;
    /** Whether this Feature is an Edge with no inward or outward indentation. */
    fn is_edge(&self) -> bool;
    /** Whether this Feature is an Edge with no inward or outward indentation. */
    fn is_convex_corner(&self) -> bool;
    /** Whether this Feature is a concave corner (inward indentation in a shape). */
    fn is_concave_corner(&self) -> bool;
}

/**
 * Edges have only a list of the cubic curves which make up the edge. Edges lie between corners
 * and have no vertex or concavity; the curves are simply straight lines (represented by Cubic
 * curves).
 */
#[derive(Clone, Debug, Hash, Eq, PartialEq)]
pub struct Edge {
    pub cubics: Vec<Cubic>
}

impl Edge {
    pub fn new(cubics: Vec<Cubic>) -> Self {
        Edge { cubics }
    }
}
impl FeatureTrait for Edge {
    fn cubics(&self) -> &Vec<Cubic> {
        &self.cubics
    }

    fn transformed(&self, f: &PointTransformer) -> Feature {
        let new_cubics: Vec<Cubic> = self.cubics.iter().map(|c| {
            let c = c.transformed(f);
            c
        }).collect();
        Feature::edge(
            new_cubics
        )
    }

    fn reversed(&self) -> Feature {
        Feature::edge(
            self.cubics.iter().rev().map(|c| c.reverse()).collect()
        )
    }

    fn is_ignorable_feature(&self) -> bool {
        true
    }

    fn is_edge(&self) -> bool {
        true
    }

    fn is_convex_corner(&self) -> bool {
        false
    }

    fn is_concave_corner(&self) -> bool {
        false
    }
}

/**
 * Corners contain the list of cubic curves which describe how the corner is rounded (or not),
 * and a flag indicating whether the corner is convex. A regular polygon has all convex corners,
 * while a star polygon generally (but not necessarily) has both convex (outer) and concave
 * (inner) corners.
 */
#[derive(Clone, Debug, Hash, Eq, PartialEq)]
pub struct Corner {
    pub cubics: Vec<Cubic>,
    pub convex: bool,
}
impl Corner {
    pub fn new(cubics: Vec<Cubic>, convex: impl Into<Option<bool>>) -> Self {
        Corner {
            cubics,
            convex: convex.into().unwrap_or(true),
        }
    }
}

impl FeatureTrait for Corner {
    fn cubics(&self) -> &Vec<Cubic> {
        &self.cubics
    }

    fn transformed(&self, f: &PointTransformer) -> Feature {
        Feature::corner(
            self.cubics.iter().map(|c| c.transformed(f)).collect(),
            self.convex
        )
    }

    fn reversed(&self) -> Feature {
        Feature::corner(
            self.cubics.iter().rev().map(|c| c.reverse()).collect(),
            // TODO: b/369320447 - Revert flag negation when [RoundedPolygon] ignores orientation
            // for setting the flag
            !self.convex
        )
    }

    fn is_ignorable_feature(&self) -> bool {
        false
    }

    fn is_edge(&self) -> bool {
        false
    }

    fn is_convex_corner(&self) -> bool {
        self.convex
    }

    fn is_concave_corner(&self) -> bool {
        !self.convex
    }
}

#[cfg(test)]
mod feature_tests {
    use std::panic::catch_unwind;
    use crate::assert_panic;
    use crate::cubic::Cubic;
    use crate::feature::{Feature, FeatureFactory};

    #[test]
    fn cannot_build_empty_features() {
        assert_panic!({FeatureFactory::build_convex_corner(vec![])});
        assert_panic!({FeatureFactory::build_concave_corner(vec![])});
        assert_panic!({FeatureFactory::build_ignorable_feature(vec![])});
    }

    #[test]
    fn cannot_build_non_continuous_features() {
        let cubic1 = Cubic::straight_line(0.0, 0.0, 1.0, 1.0);
        let cubic2 = Cubic::straight_line(10.0, 10.0, 11.0, 11.0);
        assert_panic!({FeatureFactory::build_convex_corner(vec![cubic1.clone(), cubic2.clone()])});
        assert_panic!({FeatureFactory::build_concave_corner(vec![cubic1.clone(), cubic2.clone()])});
        assert_panic!({FeatureFactory::build_ignorable_feature(vec![cubic1.clone(), cubic2.clone()])});
    }
    #[test]
    fn builds_concave_corner() {
        let cubic = Cubic::straight_line(0.0, 0.0, 1.0, 0.0);
        let actual = FeatureFactory::build_concave_corner(vec![cubic.clone()]);
        let expected = Feature::corner(vec![cubic], false);
        assert_eq!(actual, expected);
    }
    #[test]
    fn builds_convex_corner() {
        let cubic = Cubic::straight_line(0.0, 0.0, 1.0, 0.0);
        let actual = FeatureFactory::build_convex_corner(vec![cubic.clone()]);
        let expected = Feature::corner(vec![cubic], true);
        assert_eq!(actual, expected);
    }
    #[test]
    fn builds_edge() {
        let cubic = Cubic::straight_line(0.0, 0.0, 1.0, 0.0);
        let actual = FeatureFactory::build_edge(cubic.clone());
        let expected = Feature::edge(vec![cubic]);
        assert_eq!(actual, expected);
    }
    #[test]
    fn builds_ignorable_as_edge() {
        let cubic = Cubic::straight_line(0.0, 0.0, 1.0, 0.0);
        let actual = FeatureFactory::build_ignorable_feature(vec![cubic.clone()]);
        let expected = Feature::edge(vec![cubic]);
        assert_eq!(actual, expected);
    }
}