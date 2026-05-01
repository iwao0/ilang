use std::collections::HashMap;

use ilang_ast::{Block, Expr, FnDecl, Item, Param, Program, Stmt, Type, UnOp};

use crate::error::TypeError;
use crate::ops::{assignable, bin_result};

#[derive(Debug, Clone)]
struct Signature {
    params: Vec<Type>,
    ret: Type,
}

type Vars = HashMap<String, Type>;

#[derive(Debug, Default)]
pub struct TypeChecker {
    fns: HashMap<String, Signature>,
    /// Persistent top-level variable bindings — needed by the REPL so a `let`
    /// on one line is still in scope on the next.
    vars: HashMap<String, Type>,
}

impl TypeChecker {
    pub fn new() -> Self {
        Self::default()
    }

    /// Type-check a program. Top-level `fn` items are registered first so
    /// functions can reference each other regardless of declaration order.
    /// Returns the type of the program's result (tail expression, last stmt,
    /// or `Unit`).
    pub fn check(&mut self, prog: &Program) -> Result<Type, TypeError> {
        for item in &prog.items {
            match item {
                Item::Fn(f) => {
                    let sig = Signature {
                        params: f.params.iter().map(|p| p.ty).collect(),
                        ret: f.ret.unwrap_or(Type::Unit),
                    };
                    self.fns.insert(f.name.clone(), sig);
                }
            }
        }
        for item in &prog.items {
            match item {
                Item::Fn(f) => self.check_fn(f)?,
            }
        }

        // Top-level `let`s persist across calls (REPL); a temp env starts
        // from the persistent vars and the additions are merged back at the
        // end iff the whole check succeeded.
        let mut env: Vars = self.vars.clone();
        let mut last = Type::Unit;
        for s in &prog.stmts {
            last = self.check_stmt(s, &mut env)?;
        }
        if let Some(t) = &prog.tail {
            last = self.check_expr(t, &env)?;
        }
        self.vars = env;
        Ok(last)
    }

    fn check_fn(&self, f: &FnDecl) -> Result<(), TypeError> {
        let mut env: Vars = HashMap::new();
        for Param { name, ty } in &f.params {
            env.insert(name.clone(), *ty);
        }
        let body_ty = self.check_block(&f.body, &env)?;
        let expected = f.ret.unwrap_or(Type::Unit);
        if !assignable(body_ty, expected) {
            return Err(TypeError::BadReturn {
                name: f.name.clone(),
                expected,
                got: body_ty,
            });
        }
        Ok(())
    }

    fn check_block(&self, block: &Block, outer: &Vars) -> Result<Type, TypeError> {
        let mut env = outer.clone();
        let mut last = Type::Unit;
        for s in &block.stmts {
            last = self.check_stmt(s, &mut env)?;
        }
        if let Some(t) = &block.tail {
            last = self.check_expr(t, &env)?;
        }
        Ok(last)
    }

    fn check_stmt(&self, stmt: &Stmt, env: &mut Vars) -> Result<Type, TypeError> {
        match stmt {
            Stmt::Let { name, ty, value } => {
                let vt = self.check_expr(value, env)?;
                let bind = match ty {
                    Some(ann) => {
                        if !assignable(vt, *ann) {
                            return Err(TypeError::Mismatch {
                                expected: *ann,
                                got: vt,
                            });
                        }
                        *ann
                    }
                    None => vt,
                };
                env.insert(name.clone(), bind);
                Ok(Type::Unit)
            }
            Stmt::Expr(e) => self.check_expr(e, env),
        }
    }

    fn check_expr(&self, expr: &Expr, env: &Vars) -> Result<Type, TypeError> {
        match expr {
            Expr::Int(_) => Ok(Type::I64),
            Expr::Float(_) => Ok(Type::F64),
            Expr::Bool(_) => Ok(Type::Bool),
            Expr::Var(n) => env
                .get(n)
                .copied()
                .ok_or_else(|| TypeError::UndefinedVariable(n.clone())),
            Expr::Unary { op, expr } => {
                let t = self.check_expr(expr, env)?;
                match (op, t) {
                    (UnOp::Neg | UnOp::Pos, Type::I64) => Ok(Type::I64),
                    (UnOp::Neg | UnOp::Pos, Type::F64) => Ok(Type::F64),
                    (UnOp::Not, Type::Bool) => Ok(Type::Bool),
                    (_, other) => Err(TypeError::BadUnary(other)),
                }
            }
            Expr::Binary { op, lhs, rhs } => {
                let l = self.check_expr(lhs, env)?;
                let r = self.check_expr(rhs, env)?;
                bin_result(*op, l, r)
            }
            Expr::Logical { op: _, lhs, rhs } => {
                let l = self.check_expr(lhs, env)?;
                let r = self.check_expr(rhs, env)?;
                if l != Type::Bool || r != Type::Bool {
                    return Err(TypeError::BadBinary(l, r));
                }
                Ok(Type::Bool)
            }
            Expr::Call { callee, args } => {
                let sig = self
                    .fns
                    .get(callee)
                    .cloned()
                    .ok_or_else(|| TypeError::UndefinedFunction(callee.clone()))?;
                if sig.params.len() != args.len() {
                    return Err(TypeError::ArityMismatch {
                        name: callee.clone(),
                        expected: sig.params.len(),
                        got: args.len(),
                    });
                }
                for (param_ty, arg) in sig.params.iter().zip(args.iter()) {
                    let at = self.check_expr(arg, env)?;
                    if !assignable(at, *param_ty) {
                        return Err(TypeError::Mismatch {
                            expected: *param_ty,
                            got: at,
                        });
                    }
                }
                Ok(sig.ret)
            }
            Expr::Block(b) => self.check_block(b, env),
            Expr::If {
                cond,
                then_branch,
                else_branch,
            } => {
                let c = self.check_expr(cond, env)?;
                if c != Type::Bool {
                    return Err(TypeError::Mismatch {
                        expected: Type::Bool,
                        got: c,
                    });
                }
                let then_ty = self.check_block(then_branch, env)?;
                match else_branch {
                    None => {
                        // `if` without `else` produces Unit and the then-branch
                        // must also be Unit (otherwise its value is discarded).
                        if then_ty != Type::Unit {
                            return Err(TypeError::Mismatch {
                                expected: Type::Unit,
                                got: then_ty,
                            });
                        }
                        Ok(Type::Unit)
                    }
                    Some(else_e) => {
                        let else_ty = self.check_expr(else_e, env)?;
                        if then_ty == else_ty {
                            Ok(then_ty)
                        } else if assignable(then_ty, else_ty) {
                            Ok(else_ty)
                        } else if assignable(else_ty, then_ty) {
                            Ok(then_ty)
                        } else {
                            Err(TypeError::Mismatch {
                                expected: then_ty,
                                got: else_ty,
                            })
                        }
                    }
                }
            }
            Expr::While { cond, body } => {
                let c = self.check_expr(cond, env)?;
                if c != Type::Bool {
                    return Err(TypeError::Mismatch {
                        expected: Type::Bool,
                        got: c,
                    });
                }
                // Body's value is discarded; require Unit so authors don't
                // accidentally produce values that go nowhere.
                let body_ty = self.check_block(body, env)?;
                if body_ty != Type::Unit {
                    return Err(TypeError::Mismatch {
                        expected: Type::Unit,
                        got: body_ty,
                    });
                }
                Ok(Type::Unit)
            }
            Expr::Assign { target, value } => {
                let var_ty = env
                    .get(target)
                    .copied()
                    .ok_or_else(|| TypeError::UndefinedVariable(target.clone()))?;
                let v_ty = self.check_expr(value, env)?;
                if !assignable(v_ty, var_ty) {
                    return Err(TypeError::Mismatch {
                        expected: var_ty,
                        got: v_ty,
                    });
                }
                Ok(Type::Unit)
            }
        }
    }
}
