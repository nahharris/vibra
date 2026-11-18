//! Checked definition node projection and SCC address construction.

use super::{
    ComponentAddress, EdgeKind, EdgeTarget, EncodedComponent, EncodedEdge,
    EncodedObject, EntityKey, EntityMapping, GraphEncodingError, ObjectAddress,
    ObjectKey, ObjectKind, ObjectOrigin, address_from_digest, hash_domain,
};

mod address;
mod tree;

#[cfg(test)]
pub(super) use address::EncodedMembers;
pub(super) use address::{
    Candidate, CandidateReference, CandidateTarget, canonical_frame, encode_candidates,
};
pub(super) use tree::{
    ContractDispatch, DefinitionNode, IrProjection, PendingReference, array, atom,
    encode_definition, normalized_type_text, string, tagged,
};
#[cfg(test)]
mod tests {
    use super::*;

    fn function(name: &str, value: &str) -> Candidate {
        function_at(&format!("@demo@0.1.0/app.main.{name}"), value)
    }

    fn function_at(identity: &str, value: &str) -> Candidate {
        Candidate::new(
            ObjectKey::Declaration {
                identity: identity.to_owned(),
                entity_kind: "function".to_owned(),
            },
            ObjectKind::Defn,
            &DefinitionNode::List(vec![
                DefinitionNode::Atom("body".to_owned()),
                DefinitionNode::String(value.to_owned()),
            ]),
        )
        .expect("valid candidate")
    }

    fn call(target: &ObjectKey) -> CandidateReference {
        CandidateReference {
            part: Vec::new(),
            kind: EdgeKind::Call,
            target: CandidateTarget::Object(target.clone()),
            dispatch: None,
        }
    }

    fn address_for(graph: &EncodedMembers, name: &str) -> String {
        graph
            .objects
            .iter()
            .find(|object| object.key.canonical().contains(&format!(".{name}\"")))
            .map(|object| object.address.as_str().to_owned())
            .expect("object address")
    }

    #[test]
    fn candidate_insertion_order_does_not_change_component_or_object_addresses() {
        let mut left = function("left", "l");
        let right = function("right", "r");
        let left_key = left.key.clone();
        left.references.push(call(&right.key));
        let expected = encode_candidates(vec![left.clone(), right.clone()])
            .expect("graph encodes");
        let reversed = encode_candidates(vec![right, left]).expect("graph encodes");
        assert_eq!(expected, reversed);
        assert_eq!(
            address_for(&expected, "left"),
            address_for(&reversed, "left")
        );
        assert_eq!(left_key, expected.objects[0].key);
    }

    #[test]
    fn self_and_mutual_recursion_form_finite_canonical_components() {
        let mut self_recursive = function("self", "body");
        self_recursive
            .references
            .push(call(&self_recursive.key.clone()));
        let graph = encode_candidates(vec![self_recursive]).expect("self SCC");
        assert_eq!(graph.components.len(), 1);
        assert_eq!(graph.components[0].members.len(), 1);
        assert!(
            String::from_utf8_lossy(&graph.components[0].payload)
                .contains("@component-member index: 0u64")
        );

        let mut alpha = function("alpha", "a");
        let mut beta = function("beta", "b");
        alpha.references.push(call(&beta.key));
        beta.references.push(call(&alpha.key));
        let mutual = encode_candidates(vec![beta, alpha]).expect("mutual SCC");
        assert_eq!(mutual.components.len(), 1);
        assert_eq!(mutual.components[0].members.len(), 2);
    }

    #[test]
    fn duplicate_reference_occurrences_have_one_payload_and_edge_row() {
        let mut caller = function("caller", "body");
        let target = function("target", "body");
        caller.references.push(call(&target.key));
        caller.references.push(call(&target.key));

        let graph = encode_candidates(vec![caller, target]).expect("graph encodes");
        assert_eq!(graph.edges.len(), 1);
        let caller = graph
            .objects
            .iter()
            .find(|object| object.key.canonical().contains(".caller\""))
            .expect("caller object");
        let component = graph
            .components
            .iter()
            .find(|component| component.address == caller.component_address)
            .expect("caller component");
        let payload = String::from_utf8_lossy(&component.payload);
        assert_eq!(payload.matches("kind: @call").count(), 1);
    }

    #[test]
    fn changed_dependency_changes_dependents_but_not_unrelated_components() {
        let mut dependent = function("dependent", "same");
        let dependency = function("dependency", "v1");
        dependent.references.push(call(&dependency.key));
        let unrelated = function("unrelated", "stable");
        let before = encode_candidates(vec![
            dependent.clone(),
            dependency.clone(),
            unrelated.clone(),
        ])
        .expect("graph encodes");
        let mut changed_dependency = function("dependency", "v2");
        changed_dependency.key = dependency.key.clone();
        let after = encode_candidates(vec![dependent, changed_dependency, unrelated])
            .expect("graph encodes");
        assert_ne!(
            address_for(&before, "dependent"),
            address_for(&after, "dependent")
        );
        assert_ne!(
            address_for(&before, "dependency"),
            address_for(&after, "dependency")
        );
        assert_eq!(
            address_for(&before, "unrelated"),
            address_for(&after, "unrelated")
        );
    }

    #[test]
    fn nominal_keys_do_not_collapse_equal_definitions_and_missing_refs_fail() {
        let base = function("same", "body");
        let other_nominal = function_at("@demo@0.1.0/app.other.same", "body");
        assert_eq!(base.definition, other_nominal.definition);
        let graph = encode_candidates(vec![base.clone(), other_nominal])
            .expect("nominal objects remain distinct");
        assert_eq!(graph.objects.len(), 2);
        assert_ne!(graph.objects[0].address, graph.objects[1].address);

        let mut dangling = function("dangling", "body");
        dangling.references.push(call(&base.key));
        assert!(matches!(
            encode_candidates(vec![dangling]),
            Err(GraphEncodingError::MissingResolution(_))
        ));
    }
}
