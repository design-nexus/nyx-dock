.pragma library

// Plugin CSS for QML: rules as a host compiles them (nyx-bar-backend's css.rs):
//   { sel: [{type, classes, not, hover, root}, …], spec, props: {…} }
// A selector's last compound is the node itself; each earlier one an ancestor (any
// distance), `root` being the item's own root (the host's element, `&` in the CSS).
// `node` is { kind, classes, hovered, up } — `up` the parent node, `null` above the
// view's top, where the host's root stands in as { root: true, classes, hovered }.

function typeOf(kind) {
    if (kind === "icon" || kind === "image")
        return "image"
    if (kind === "overlay" || kind === "stack" || kind === "scroll")
        return "box"
    return kind
}

function matches(c, el) {
    if (c.root && !el.root)
        return false
    if (!c.root && el.root && c.type)
        return false
    if (c.type && (el.root || typeOf(el.kind) !== c.type))
        return false
    if (c.hover && !el.hovered)
        return false
    const cls = el.classes || []
    for (let i = 0; i < c.classes.length; i++)
        if (cls.indexOf(c.classes[i]) < 0)
            return false
    for (let i = 0; i < (c.not || []).length; i++)
        if (cls.indexOf(c.not[i]) >= 0)
            return false
    return true
}

// The chain from the node up: the node, its ancestors, then the host's root.
function ruleApplies(rule, chain) {
    const sel = rule.sel
    let i = sel.length - 1
    if (!matches(sel[i], chain[0]))
        return false
    i--
    for (let j = 1; j < chain.length && i >= 0; j++) {
        if (matches(sel[i], chain[j]))
            i--
    }
    return i < 0
}

// The properties for a node: matching rules by specificity, then order.
function compute(rules, chain) {
    if (!rules || rules.length === 0)
        return ({})
    const hits = []
    for (let i = 0; i < rules.length; i++)
        if (ruleApplies(rules[i], chain))
            hits.push({ spec: rules[i].spec || 0, order: i, props: rules[i].props })
    if (hits.length === 0)
        return ({})
    hits.sort((a, b) => a.spec - b.spec || a.order - b.order)
    const out = {}
    for (const h of hits)
        for (const k in h.props)
            out[k] = h.props[k]
    return out
}

// Whether any rule asks for :hover (only then do nodes track the pointer).
function anyHover(rules) {
    for (const r of rules || [])
        for (const c of r.sel)
            if (c.hover)
                return true
    return false
}
