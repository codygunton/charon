//! `--devirtualize <Trait>=<Type>`: replace the trait object type `dyn <Trait>` with `<Type>`,
//! the only implementation of `<Trait>` that the crate ever turns into a trait object.
//!
//! For each mapping:
//! - every unsizing cast to `dyn <Trait>` must start from `<Type>` (behind a reference, raw
//!   pointer, `Box` or other pointer ADT); any other source type is a fatal error. These casts
//!   become plain moves;
//! - every `dyn <Trait>` type becomes `<Type>`;
//! - every `dyn <Trait>` trait proof becomes the `<Type>: <Trait>` proof recorded at the
//!   unsizing casts, so that `skip_trait_refs_when_known` turns vtable method calls into direct
//!   calls to `<Type>`'s impl methods;
//! - the vtable instances, their initializers and vtable shims that are no longer used are
//!   removed.
//!
//! `<Trait>` is a name pattern. `<Type>` is a name pattern, or `()` for the unit type.
use std::collections::{HashMap, HashSet};

use derive_generic_visitor::*;
use itertools::Itertools;

use crate::errors::Level;
use crate::formatter::IntoFormatter;
use crate::name_matcher::NamePattern;
use crate::pretty::FmtWithCtx;
use crate::transform::{TransformCtx, ctx::TransformPass};
use crate::{ast::*, llbc_ast, ullbc_ast};

enum ConcretePattern {
    Unit,
    Path(NamePattern),
}

struct Mapping {
    /// The option as written by the user, for error messages.
    trait_str: String,
    concrete_str: String,
    concrete: ConcretePattern,
    /// The traits matched by the trait pattern.
    traits: HashSet<TraitDeclId>,
    /// The `<Type>: <Trait>` proof per principal trait, recorded at the unsizing casts (or found
    /// among the trait impls).
    proofs: HashMap<TraitDeclId, TraitRef>,
}

/// Parse a `--devirtualize` argument.
pub fn parse_mapping(arg: &str) -> Result<(NamePattern, Option<NamePattern>), String> {
    let Some((trait_str, concrete_str)) = arg.rsplit_once('=') else {
        return Err(format!(
            "`--devirtualize {arg}`: expected `<dyn-trait-path>=<concrete-type>`"
        ));
    };
    let trait_pat = NamePattern::parse(trait_str.trim())
        .map_err(|e| format!("`--devirtualize {arg}`: invalid trait pattern: {e}"))?;
    let concrete = match concrete_str.trim() {
        "()" => None,
        s => Some(
            NamePattern::parse(s)
                .map_err(|e| format!("`--devirtualize {arg}`: invalid type pattern: {e}"))?,
        ),
    };
    Ok((trait_pat, concrete))
}

/// The principal (method-carrying) trait of a `dyn` type.
fn principal_trait(ty: &Ty) -> Option<TraitDeclId> {
    let TyKind::DynTrait(pred) = ty.kind() else {
        return None;
    };
    pred.binder
        .params
        .trait_clauses
        .first()
        .map(|clause| clause.trait_.skip_binder.id)
}

/// Given the source and target types of an unsizing cast, find the pair (concrete type, `dyn`
/// type) that the cast unsizes, looking through pointers and pointer-like ADTs.
fn unsized_pair(src: &Ty, tgt: &Ty) -> Option<(Ty, Ty)> {
    if src == tgt {
        return None;
    }
    if let TyKind::DynTrait(_) = tgt.kind() {
        return Some((src.clone(), tgt.clone()));
    }
    match (src.kind(), tgt.kind()) {
        (TyKind::Ref(_, s, _), TyKind::Ref(_, t, _))
        | (TyKind::RawPtr(s, _), TyKind::RawPtr(t, _)) => unsized_pair(s, t),
        (TyKind::Adt(s), TyKind::Adt(t)) if s.id == t.id => s
            .generics
            .types
            .iter()
            .zip(t.generics.types.iter())
            .find_map(|(s, t)| unsized_pair(s, t)),
        _ => None,
    }
}

fn fmt_span(krate: &TranslatedCrate, span: Span) -> String {
    let data = span.data();
    match krate.files.get(data.file_id) {
        Some(file) => format!("{}:{}:{}", file.name, data.beg.line, data.beg.col + 1),
        None => "<unknown location>".to_owned(),
    }
}

/// First phase: check the unsizing casts, record the proofs they carry and turn them into moves.
#[derive(Visitor)]
struct CastVisitor<'a> {
    ctx: &'a TransformCtx,
    mappings: &'a mut [Mapping],
    span: Span,
    fatal: bool,
}

impl CastVisitor<'_> {
    fn mapping_for(&self, dyn_ty: &Ty) -> Option<usize> {
        let id = principal_trait(dyn_ty)?;
        self.mappings.iter().position(|m| m.traits.contains(&id))
    }

    fn concrete_matches(&self, m: &Mapping, ty: &Ty) -> bool {
        match &m.concrete {
            ConcretePattern::Unit => ty.is_unit(),
            ConcretePattern::Path(pat) => pat.matches_ty(&self.ctx.translated, ty),
        }
    }

    fn check_cast(&mut self, rvalue: &mut Rvalue) {
        let Rvalue::UnaryOp(UnOp::Cast(CastKind::Unsize(tgt_ty, meta, _)), operand) = rvalue else {
            return;
        };
        let fmt = &self.ctx.into_fmt();
        let src_ty = operand.ty();
        let Some((concrete, dyn_ty)) = unsized_pair(src_ty, tgt_ty) else {
            return;
        };
        // `dyn Trait + 'a` to `dyn Trait + 'b` (or to fewer auto traits): both sides become the
        // concrete type.
        if principal_trait(&concrete).is_some()
            && principal_trait(&concrete) == principal_trait(&dyn_ty)
            && self.mapping_for(&dyn_ty).is_some()
        {
            *rvalue = Rvalue::Use(operand.clone(), WithRetag::No);
            return;
        }
        let Some(i) = self.mapping_for(&dyn_ty) else {
            // Upcasting from a devirtualized trait object to another one.
            if let Some(j) = self.mapping_for(&concrete) {
                self.ctx.span_err(
                    self.span,
                    &format!(
                        "devirtualize: upcast of {} to {} at {} is not supported for `dyn {}`",
                        src_ty.with_ctx(fmt),
                        tgt_ty.with_ctx(fmt),
                        fmt_span(&self.ctx.translated, self.span),
                        self.mappings[j].trait_str,
                    ),
                    Level::ERROR,
                );
                self.fatal = true;
            }
            return;
        };
        let m = &self.mappings[i];
        let proof = match meta {
            UnsizingMetadata::VTable(proof, _) => Some(proof.clone().erase()),
            _ => None,
        };
        let Some(proof) = proof.filter(|_| self.concrete_matches(m, &concrete)) else {
            self.ctx.span_err(
                self.span,
                &format!(
                    "devirtualize: unsizing of {} to dyn {} at {} does not match configured \
                     implementation {}",
                    concrete.with_ctx(fmt),
                    m.trait_str,
                    fmt_span(&self.ctx.translated, self.span),
                    m.concrete_str,
                ),
                Level::ERROR,
            );
            self.fatal = true;
            return;
        };
        let principal = principal_trait(&dyn_ty).unwrap();
        self.mappings[i].proofs.entry(principal).or_insert(proof);
        *rvalue = Rvalue::Use(operand.clone(), WithRetag::No);
    }
}

impl VisitAstMut for CastVisitor<'_> {
    fn enter_ullbc_statement(&mut self, st: &mut ullbc_ast::Statement) {
        self.span = st.span;
    }
    fn enter_ullbc_terminator(&mut self, t: &mut ullbc_ast::Terminator) {
        self.span = t.span;
    }
    fn enter_llbc_statement(&mut self, st: &mut llbc_ast::Statement) {
        self.span = st.span;
    }
    fn exit_rvalue(&mut self, rvalue: &mut Rvalue) {
        self.check_cast(rvalue);
    }
}

/// Second phase: replace the `dyn` types and their proofs.
#[derive(Visitor)]
struct ReplaceVisitor<'a> {
    ctx: &'a TransformCtx,
    /// Principal trait -> (concrete type, proof of `concrete: Trait`).
    targets: &'a HashMap<TraitDeclId, (Ty, TraitRef, &'a Mapping)>,
    destruct: Option<TraitDeclId>,
    /// Types are hash-consed and appear many times; remember what each visited type became.
    visited_tys: &'a mut HashMap<Ty, Ty>,
    /// The method implementations of the non-generic impls used as proofs. With
    /// `--monomorphize`, impls don't list their methods, so `skip_trait_refs_when_known` can't
    /// turn the former vtable calls into direct calls; we do it here.
    impl_methods: &'a HashMap<(TraitImplId, TraitMethodId), FunDeclId>,
}

impl ReplaceVisitor<'_> {
    fn target(&self, ty: &Ty) -> Option<&(Ty, TraitRef, &Mapping)> {
        self.targets.get(&principal_trait(ty)?)
    }
}

impl VisitAstMut for ReplaceVisitor<'_> {
    fn visit_ty(&mut self, ty: &mut Ty) -> ControlFlow<Self::Break> {
        if let Some(new_ty) = self.visited_tys.get(ty) {
            *ty = new_ty.clone();
            return Continue(());
        }
        let old_ty = ty.clone();
        if let Some((concrete, _, _)) = self.target(ty) {
            *ty = concrete.clone();
        } else {
            self.visit_inner(ty)?;
        }
        self.visited_tys.insert(old_ty, ty.clone());
        Continue(())
    }

    fn exit_fn_ptr(&mut self, fn_ptr: &mut FnPtr) {
        let FnPtrKind::Trait(tref, method_id) = fn_ptr.kind.as_ref() else {
            return;
        };
        let TraitRefKind::TraitImpl(impl_ref) = &tref.kind else {
            return;
        };
        if let Some(&fun_id) = self.impl_methods.get(&(impl_ref.id, *method_id)) {
            *fn_ptr = FnPtr::new(FnPtrKind::Fun(fun_id), fn_ptr.generics.clone());
        }
    }

    fn enter_trait_ref(&mut self, tref: &mut TraitRef) {
        if !matches!(tref.kind, TraitRefKind::Dyn) {
            return;
        }
        let Some(self_ty) = tref.trait_decl_ref.generics.types.get(TypeVarId::ZERO) else {
            return;
        };
        let Some((concrete, proof, m)) = self.target(self_ty) else {
            return;
        };
        let new_kind = if tref.trait_id() == proof.trait_id() {
            proof.kind.clone()
        } else if Some(tref.trait_id()) == self.destruct {
            // Dropping the trait object drops the concrete value.
            TraitRefKind::BuiltinOrAuto {
                builtin_data: if concrete.is_unit() {
                    BuiltinImplData::NoopDestruct
                } else {
                    BuiltinImplData::UntrackedDestruct
                },
                parent_trait_refs: Default::default(),
                types: Default::default(),
                vtable: None,
            }
        } else if let TyKind::DynTrait(pred) = self_ty.kind()
            && pred
                .binder
                .params
                .trait_clauses
                .iter()
                .skip(1)
                .any(|c| c.trait_.skip_binder.id == tref.trait_id())
        {
            // The other traits of a `dyn` type are auto traits (`dyn Trait + Send + Sync`).
            TraitRefKind::BuiltinOrAuto {
                builtin_data: BuiltinImplData::Auto,
                parent_trait_refs: Default::default(),
                types: Default::default(),
                vtable: None,
            }
        } else {
            let fmt = &self.ctx.into_fmt();
            self.ctx.span_err(
                Span::dummy(),
                &format!(
                    "devirtualize: no proof of {} for {} (`dyn {}` replaced by {})",
                    tref.trait_id().with_ctx(fmt),
                    concrete.with_ctx(fmt),
                    m.trait_str,
                    m.concrete_str,
                ),
                Level::WARNING,
            );
            return;
        };
        tref.with_contents_mut(|c| c.kind = new_kind);
    }
}

pub struct Transform;
impl TransformPass for Transform {
    fn transform_ctx(&self, ctx: &mut TransformCtx) {
        if ctx.options.devirtualize.is_empty() {
            return;
        }
        let mut mappings: Vec<Mapping> = vec![];
        for arg in &ctx.options.devirtualize {
            // Validated in `CliOpts::validate`.
            let (trait_pat, concrete) = parse_mapping(arg).unwrap();
            let (trait_str, concrete_str) = arg.rsplit_once('=').unwrap();
            let traits: HashSet<TraitDeclId> = ctx
                .translated
                .trait_decls
                .iter()
                .filter(|d| trait_pat.matches(&ctx.translated, &d.item_meta.name))
                .map(|d| d.def_id)
                .collect();
            mappings.push(Mapping {
                trait_str: trait_str.trim().to_owned(),
                concrete_str: concrete_str.trim().to_owned(),
                concrete: match concrete {
                    None => ConcretePattern::Unit,
                    Some(pat) => ConcretePattern::Path(pat),
                },
                traits,
                proofs: Default::default(),
            });
        }

        // Phase 1: unsizing casts.
        let mut fatal = false;
        ctx.for_each_item_mut(|ctx, mut item| {
            let mut v = CastVisitor {
                ctx,
                mappings: &mut mappings,
                span: Span::dummy(),
                fatal: false,
            };
            let _ = item.drive_mut(&mut v);
            fatal |= v.fatal;
        });
        if fatal {
            ctx.errors.borrow_mut().fatal = true;
            return;
        }

        // Traits that are devirtualized without any unsizing cast in the crate (e.g. trait
        // objects that only flow in from outside): use the unique non-generic impl for the type.
        for m in mappings.iter_mut() {
            for &trait_id in &m.traits {
                if m.proofs.contains_key(&trait_id) {
                    continue;
                }
                let candidates = ctx
                    .translated
                    .trait_impls
                    .iter()
                    .filter(|imp| imp.impl_trait.id == trait_id && imp.generics.is_empty())
                    .filter(|imp| {
                        let self_ty = &imp.impl_trait.generics.types[TypeVarId::ZERO];
                        match &m.concrete {
                            ConcretePattern::Unit => self_ty.is_unit(),
                            ConcretePattern::Path(pat) => pat.matches_ty(&ctx.translated, self_ty),
                        }
                    })
                    .collect_vec();
                if let [imp] = candidates.as_slice() {
                    let proof = TraitRef::new(
                        TraitRefKind::TraitImpl(TraitImplRef {
                            id: imp.def_id,
                            generics: Box::new(GenericArgs::empty()),
                        }),
                        imp.impl_trait.clone(),
                    );
                    m.proofs.insert(trait_id, proof);
                }
            }
        }

        // Phase 2: types and proofs.
        let mut targets: HashMap<TraitDeclId, (Ty, TraitRef, &Mapping)> = HashMap::new();
        for m in &mappings {
            for (&trait_id, proof) in &m.proofs {
                let concrete = proof.trait_decl_ref.generics.types[TypeVarId::ZERO].clone();
                targets.insert(trait_id, (concrete, proof.clone(), m));
            }
        }
        let destruct = ctx
            .translated
            .trait_decls
            .iter()
            .find(|d| d.item_meta.lang_item == Some(from_rustc::LangItem::Destruct))
            .map(|d| d.def_id);
        let proof_impls: HashSet<TraitImplId> = targets
            .values()
            .filter_map(|(_, proof, _)| match &proof.kind {
                TraitRefKind::TraitImpl(impl_ref) if impl_ref.generics.is_empty() => {
                    Some(impl_ref.id)
                }
                _ => None,
            })
            .collect();
        let impl_methods: HashMap<(TraitImplId, TraitMethodId), FunDeclId> = ctx
            .translated
            .fun_decls
            .iter()
            .filter_map(|f| match &f.src {
                FunSource::TraitImpl {
                    impl_ref, item_id, ..
                } if proof_impls.contains(&impl_ref.id) => {
                    Some(((impl_ref.id, *item_id), f.def_id))
                }
                _ => None,
            })
            .collect();
        let mut visited_tys = HashMap::new();
        ctx.for_each_item_mut(|ctx, mut item| {
            let mut v = ReplaceVisitor {
                ctx,
                targets: &targets,
                destruct,
                visited_tys: &mut visited_tys,
                impl_methods: &impl_methods,
            };
            let _ = item.drive_mut(&mut v);
        });

        // Remaining `dyn` types of a configured trait have no implementation to use.
        let devirtualized: HashSet<TraitDeclId> = targets.keys().copied().collect();
        for m in &mappings {
            for trait_id in m.traits.difference(&devirtualized) {
                let mut found = false;
                ctx.translated.dyn_visit(|ty: &Ty| {
                    found |= principal_trait(ty) == Some(*trait_id);
                });
                if found {
                    let fmt = &ctx.into_fmt();
                    ctx.span_err(
                        Span::dummy(),
                        &format!(
                            "devirtualize: `dyn {}` is used but no implementation {} was found",
                            trait_id.with_ctx(fmt),
                            m.concrete_str,
                        ),
                        Level::ERROR,
                    );
                    ctx.errors.borrow_mut().fatal = true;
                }
            }
        }

        remove_unused_vtables(ctx);
    }
}

/// Remove the vtable instances (with their initializers) and vtable shims that nothing uses
/// anymore. References from `TraitImpl::vtable` don't keep a vtable alive.
fn remove_unused_vtables(ctx: &mut TransformCtx) {
    let krate = &mut ctx.translated;
    let mut candidates: HashSet<ItemId> = HashSet::new();
    for g in krate.global_decls.iter() {
        if matches!(g.src, GlobalSource::VTableInstance { .. }) {
            candidates.insert(g.def_id.into());
            if let Some(init) = g.init_fun_id() {
                candidates.insert(init.into());
            }
        }
    }
    for f in krate.fun_decls.iter() {
        if matches!(f.src, FunSource::VTableShim) {
            candidates.insert(f.def_id.into());
        }
    }
    if candidates.is_empty() {
        return;
    }
    // Detach the impl vtables while computing liveness.
    let mut impl_vtables: HashMap<TraitImplId, VTableDecl> = HashMap::new();
    for imp in krate.trait_impls.iter_mut() {
        if let VTableDecl::VTable(_) = &imp.vtable {
            impl_vtables.insert(
                imp.def_id,
                std::mem::replace(&mut imp.vtable, VTableDecl::Lazy),
            );
        }
    }

    // The candidates that `id` refers to (other than itself).
    let refs_of = |krate: &TranslatedCrate, id: ItemId| -> Vec<ItemId> {
        let mut refs = vec![];
        if let Some(item) = krate.get_item(id) {
            let mut push = |r: ItemId| {
                if r != id && candidates.contains(&r) {
                    refs.push(r)
                }
            };
            item.dyn_visit(|x: &FunDeclId| push((*x).into()));
            item.dyn_visit(|x: &GlobalDeclId| push((*x).into()));
        }
        refs
    };
    let mut live: HashSet<ItemId> = HashSet::new();
    let mut stack: Vec<ItemId> = vec![];
    for id in krate.all_ids() {
        if candidates.contains(&id) {
            continue;
        }
        for r in refs_of(krate, id) {
            if candidates.contains(&r) && live.insert(r) {
                stack.push(r);
            }
        }
    }
    while let Some(id) = stack.pop() {
        for r in refs_of(krate, id) {
            if candidates.contains(&r) && live.insert(r) {
                stack.push(r);
            }
        }
    }
    for id in candidates.difference(&live) {
        krate.remove_item(*id);
    }
    for (impl_id, vtable) in impl_vtables {
        let keep = match &vtable {
            VTableDecl::VTable(g) => krate.global_decls.get(g.id).is_some(),
            _ => true,
        };
        if keep && let Some(imp) = krate.trait_impls.get_mut(impl_id) {
            imp.vtable = vtable;
        }
    }
}
