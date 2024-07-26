use std::collections::BTreeMap;
use std::error::Error;

use crate::ast::{Expression, FunctionCallKind, FunctionCall, NodeID};

use crate::capture;
use crate::detect::detector::IssueDetectorNamePool;
use crate::{
    context::workspace_context::{ASTNode, WorkspaceContext},
    detect::detector::{IssueDetector, IssueSeverity},
};
use eyre::Result;

#[derive(Default)]
pub struct WeakRandomnessDetector {
    // Keys are: [0] source file name, [1] line number, [2] character location of node.
    // Do not add items manually, use `capture!` to add nodes to this BTreeMap.
    found_instances: BTreeMap<(String, usize, String), NodeID>,
}

impl IssueDetector for WeakRandomnessDetector {
    fn detect(&mut self, context: &WorkspaceContext) -> Result<bool, Box<dyn Error>> {
        let keccaks: Vec<&FunctionCall> = context.function_calls()
            .into_iter()
            .filter(|x| matches!(*x.expression, Expression::Identifier(ref id) if id.name == "keccak256"))
            .collect();

        for keccak in keccaks {
            // keccak256 must have exactly one argument
            let arg = keccak.arguments.get(0).unwrap();

            if let Expression::FunctionCall(ref function_call) = *arg {
                if check_encode(function_call) {
                    capture!(self, context, keccak);
                }
            }

            // get variable definition
            else if let Expression::Identifier(ref i) = *arg {
                if let Some(node_id) = i.referenced_declaration {
                    let decleration = context.get_parent(node_id);

                    if let Some(ASTNode::VariableDeclarationStatement(var)) = decleration {
                        if let Some(Expression::FunctionCall(function_call)) = &var.initial_value {
                            if check_encode(function_call) {
                                capture!(self, context, keccak);
                            }
                        }
                    }
                }
            }
        }

        // check for modulo operations on block.timestamp, block.number and blockhash
        for binary_operation in context.binary_operations().into_iter().filter(|b| b.operator == "%") {

            // if left operand is a variable, get its definition and perform check
            if let Expression::Identifier(ref i) = *binary_operation.left_expression {
                if let Some(node_id) = i.referenced_declaration {
                    let decleration = context.get_parent(node_id);

                    if let Some(ASTNode::VariableDeclarationStatement(var)) = decleration {
                        if let Some(expression) = &var.initial_value {
                            if check_operand(expression) {
                                capture!(self, context, binary_operation);
                                continue;
                            }
                        }
                    }
                }
            }
            else if check_operand(&binary_operation.left_expression) {
                capture!(self, context, binary_operation);
            }
        }

        Ok(!self.found_instances.is_empty())
    }

    fn severity(&self) -> IssueSeverity {
        IssueSeverity::High
    }

    fn title(&self) -> String {
        String::from("Weak Randomness")
    }

    fn description(&self) -> String {
        String::from("TODO: Description of the high issue.")
    }

    fn instances(&self) -> BTreeMap<(String, usize, String), NodeID> {
        self.found_instances.clone()
    }

    fn name(&self) -> String {
        format!("{}", IssueDetectorNamePool::WeakRandomness)
    }
}

// returns whether block.timestamp or block.number is in encode function
fn check_encode(function_call: &FunctionCall) -> bool {
    if let Expression::MemberAccess(ref member_access) = *function_call.expression {
        if member_access.member_name == "encodePacked" || member_access.member_name == "encode" {
            for argument in &function_call.arguments {
                if let Expression::MemberAccess(ref member_access) = *argument {
                    if member_access.member_name == "timestamp" || member_access.member_name == "number" {
                        if let Expression::Identifier(ref identifier) =
                            *member_access.expression
                            {
                                if identifier.name == "block" {
                                    return true;
                                }
                            }
                    }
                }
            }
        }
    }
    false
}

// returns whether operand is dependent on block.timestamp, block.number or blockhash
fn check_operand(operand: &Expression) -> bool {
    match operand {
        Expression::MemberAccess(member_access) => {
            if vec!["timestamp", "number"].iter().any(|ma| {
                ma == &member_access.member_name &&
                matches!(*member_access.expression, Expression::Identifier(ref id) if id.name == "block")
            }) {
                return true;
            }
        },
        Expression::FunctionCall(function_call) => {
            if function_call.kind == FunctionCallKind::TypeConversion {
                // type conversion must have exactly one argument
                let arg = function_call.arguments.get(0).unwrap();

                if let Expression::FunctionCall(ref inner_function_call) = *arg {
                    if matches!(*inner_function_call.expression, Expression::Identifier(ref id) if id.name == "blockhash") {
                        return true;
                    }
                }
            }
        },
        _ => ()
    }

    false
}

#[cfg(test)]
mod weak_randomness_detector_tests {
    use crate::detect::{detector::IssueDetector, high::weak_randomness::WeakRandomnessDetector};

    #[test]
    fn test_weak_randomness_detector() {
        let context = crate::detect::test_utils::load_solidity_source_unit(
            "../tests/contract-playground/src/WeakRandomness.sol",
        );

        let mut detector = WeakRandomnessDetector::default();
        let found = detector.detect(&context).unwrap();
        // assert that the detector found an issue
        assert!(found);
        // assert that the detector found the correct number of instances
        assert_eq!(detector.instances().len(), 8);
        // assert the severity is high
        assert_eq!(
            detector.severity(),
            crate::detect::detector::IssueSeverity::High
        );
        // assert the title is correct
        assert_eq!(detector.title(), String::from("Weak Randomness"));
        // assert the description is correct
        assert_eq!(
            detector.description(),
            String::from("TODO: Description of the high issue.")
        );
    }
}
