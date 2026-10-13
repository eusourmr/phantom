//! Bounded JavaScript runtime for Phantom 2I-2.

use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::error::Error;
use std::fmt;

use crate::ast::{
    AssignmentOperator, BinaryOperator, Expression, ExpressionKind, ForInitializer, MemberProperty,
    Program, Statement, StatementKind, UnaryOperator, VariableDeclaration, VariableKind,
};
use crate::{ParseErrorKind, Span, parse_program};

const MAX_RUNTIME_STEPS: usize = 100_000;
const MAX_SCOPE_DEPTH: usize = 256;
const MAX_BINDINGS_PER_SCOPE: usize = 4_096;
const MAX_RUNTIME_STRING_BYTES: usize = 1024 * 1024;

// PHANTOM_2I2_OBJECTS_ARRAYS_FUNCTIONS_RUNTIME_I
const MAX_ARRAYS: usize = 4_096;
const MAX_ARRAY_LENGTH: usize = 16_384;
const MAX_OBJECTS: usize = 4_096;
const MAX_OBJECT_PROPERTIES: usize = 4_096;
const MAX_FUNCTIONS: usize = 2_048;
const MAX_FUNCTION_ARGUMENTS: usize = 1_024;
const MAX_CALL_DEPTH: usize = 128;
const MAX_RETAINED_FUNCTION_SOURCE_BYTES: usize = 8 * 1024 * 1024;
// PHANTOM_2M7_R1_FUNCTION_CONTEXT_RUNTIME_I
// Closure capture is deliberately bounded independently from ordinary scope
// depth so one function cannot retain an unbounded lexical environment.
const MAX_CAPTURED_BINDINGS_PER_FUNCTION: usize = 4_096;

// PHANTOM_2J1_BROWSER_HOST_FOUNDATION_I
const MAX_HOST_URL_BYTES: usize = 16 * 1024;
const MAX_HOST_TITLE_BYTES: usize = 64 * 1024;

// PHANTOM_2J2_LIVE_DOM_BRIDGE_I
const MAX_HOST_ELEMENTS: usize = 65_536;
const MAX_HOST_TAG_BYTES: usize = 4 * 1024;
const MAX_HOST_TEXT_BYTES: usize = 1024 * 1024;
const MAX_HOST_ATTRIBUTES_PER_ELEMENT: usize = 128;
const MAX_HOST_ATTRIBUTE_BYTES_PER_ELEMENT: usize = 1024 * 1024;
// PHANTOM_2M21_DOM_SELECTOR_API_COMPATIBILITY_II
const MAX_HOST_SELECTOR_BYTES: usize = 4 * 1024;
const MAX_HOST_SELECTOR_GROUPS: usize = 32;
const MAX_HOST_SELECTOR_STEPS: usize = 32;
const MAX_HOST_SELECTOR_TESTS_PER_COMPOUND: usize = 32;
const MAX_NATIVE_FUNCTIONS: usize = 16_384;
const MAX_DOM_MUTATIONS_PER_EXECUTION: usize = 4_096;

// PHANTOM_2J3_DOM_MUTATION_NODE_CREATION_I
const MAX_VIRTUAL_NODE_SERIAL: u64 = 1_000_000;

// PHANTOM_2J4_DOM_COLLECTIONS_HARDENING_I
const HOST_METHOD_GET_ATTRIBUTE: usize = 0;
const HOST_METHOD_SET_ATTRIBUTE: usize = 1;
const HOST_METHOD_REMOVE_ATTRIBUTE: usize = 2;
const HOST_METHOD_APPEND_CHILD: usize = 3;
const HOST_METHOD_REMOVE: usize = 4;
const HOST_METHOD_HAS_ATTRIBUTE: usize = 5;
const HOST_METHOD_QUERY_SELECTOR: usize = 6;
const HOST_METHOD_QUERY_SELECTOR_ALL: usize = 7;
const HOST_METHOD_MATCHES: usize = 8;
const HOST_METHOD_CLOSEST: usize = 9;
const HOST_METHOD_CONTAINS: usize = 10;

// PHANTOM_2M8_DOM_EVENT_TARGET_I
const HOST_METHOD_ADD_EVENT_LISTENER: usize = 11;
const HOST_METHOD_REMOVE_EVENT_LISTENER: usize = 12;
const HOST_METHOD_DISPATCH_EVENT: usize = 13;

// PHANTOM_2M12_DOM_TOKENLIST_CLASSLIST_I
const HOST_METHOD_CLASS_LIST_ADD: usize = 14;
const HOST_METHOD_CLASS_LIST_REMOVE: usize = 15;
const HOST_METHOD_CLASS_LIST_CONTAINS: usize = 16;
const HOST_METHOD_CLASS_LIST_TOGGLE: usize = 17;
const MAX_DOM_TOKEN_BYTES: usize = 4 * 1024;
const MAX_DOM_TOKEN_ARGUMENTS: usize = 128;

// PHANTOM_2M13_CSS_STYLE_DECLARATION_I
const HOST_METHOD_STYLE_SET_PROPERTY: usize = 18;
const HOST_METHOD_STYLE_GET_PROPERTY_VALUE: usize = 19;
const HOST_METHOD_STYLE_REMOVE_PROPERTY: usize = 20;
const MAX_STYLE_DECLARATIONS: usize = 512;
const MAX_STYLE_PROPERTY_NAME_BYTES: usize = 256;
const MAX_STYLE_VALUE_BYTES: usize = 64 * 1024;

// PHANTOM_2M14_DOM_DATASET_ATTRIBUTE_REFLECTION_I
const HOST_METHOD_TOGGLE_ATTRIBUTE: usize = 21;
const MAX_DOM_DATASET_KEY_BYTES: usize = 256;

// PHANTOM_2M15_HTML_ATTRIBUTE_PROPERTY_REFLECTION_II
const HOST_METHOD_GET_ATTRIBUTE_NAMES: usize = 22;

// PHANTOM_2M22_DOM_MUTATION_API_II
const HOST_METHOD_APPEND: usize = 23;
const HOST_METHOD_PREPEND: usize = 24;
const HOST_METHOD_BEFORE: usize = 25;
const HOST_METHOD_AFTER: usize = 26;
const HOST_METHOD_REPLACE_WITH: usize = 27;
const HOST_METHOD_REPLACE_CHILDREN: usize = 28;
const HOST_METHOD_REMOVE_CHILD: usize = 29;

// PHANTOM_2M23_HTML_ELEMENT_INTERACTION_I
const HOST_METHOD_CLICK: usize = 30;
const HOST_METHOD_COUNT: usize = 31;
const MAX_DOM_MUTATION_ARGUMENTS: usize = 128;

const MAX_EVENT_LISTENERS: usize = 4_096;
const MAX_EVENT_TYPE_BYTES: usize = 256;
const MAX_EVENT_DISPATCH_DEPTH: usize = 64;

// PHANTOM_2M9_DOM_EVENT_PROPAGATION_II
const MAX_EVENT_PROPAGATION_PATH: usize = 256;

// PHANTOM_2K1_EVENT_LOOP_TIMERS_I
const MAX_TIMERS: usize = 4_096;
const MAX_TIMER_DELAY_MS: u64 = 2_147_483_647;
const MAX_TIMER_TASKS_PER_PUMP: usize = 512;

// PHANTOM_2K2_MICROTASK_PROMISE_FOUNDATION_I
const MAX_MICROTASKS: usize = 4_096;
const MAX_MICROTASK_TASKS_PER_CHECKPOINT: usize = 4_096;
const MAX_PROMISES: usize = 2_048;
const MAX_PROMISE_REACTIONS_PER_PROMISE: usize = 1_024;

// PHANTOM_2K3_FETCH_ASYNC_RESOURCE_FOUNDATION_I
const MAX_FETCH_REQUESTS: usize = 256;
const MAX_FETCH_URL_BYTES: usize = MAX_HOST_URL_BYTES;
const MAX_FETCH_BODY_BYTES: usize = MAX_RUNTIME_STRING_BYTES;
const MAX_FETCH_FAILURE_BYTES: usize = 64 * 1024;
const MAX_FETCH_RESPONSES: usize = 1_024;
// PHANTOM_2N16_FETCH_REQUEST_INIT_HOST_BOUNDARY_I
const MAX_FETCH_HEADERS: usize = 64;
const MAX_FETCH_HEADER_NAME_BYTES: usize = 256;
const MAX_FETCH_HEADER_VALUE_BYTES: usize = 8 * 1024;
const MAX_FETCH_HEADER_BYTES: usize = 32 * 1024;

// PHANTOM_2M17_WEB_CORE_LIBRARY_II
const MAX_JSON_DEPTH: usize = 128;

/// Runtime value supported by Phantom JavaScript Runtime Core II.
///
/// Heap-backed arrays, objects, and functions use opaque runtime-local handles.
/// Host objects, symbols, big integers, constructors, and DOM values remain
/// deliberately deferred.
#[derive(Debug, Clone, PartialEq)]
pub enum Value {
    /// ECMAScript `undefined`.
    Undefined,
    /// ECMAScript `null`.
    Null,
    /// Boolean value.
    Boolean(bool),
    /// IEEE-754 number value.
    Number(f64),
    /// UTF-8 string value.
    String(String),
    /// Runtime-local array identity.
    Array(usize),
    /// Runtime-local ordinary-object identity.
    Object(usize),
    /// Runtime-local function identity.
    Function(usize),
    /// Runtime-local browser-host native function identity.
    NativeFunction(usize),
    /// Opaque element-bound browser-host method identity.
    ///
    /// The handle encodes the stable host object identity plus a bounded
    /// operation code; it does not allocate one native-function record per
    /// materialized DOM element.
    HostFunction(usize),
}

impl Value {
    /// Returns the JavaScript-facing type label used by Runtime Core II.
    #[must_use]
    pub const fn type_name(&self) -> &'static str {
        match self {
            Self::Undefined => "undefined",
            Self::Null => "null",
            Self::Boolean(_) => "boolean",
            Self::Number(_) => "number",
            Self::String(_) => "string",
            Self::Array(_) | Self::Object(_) => "object",
            Self::Function(_) | Self::NativeFunction(_) | Self::HostFunction(_) => "function",
        }
    }

    /// Returns the contained number when this value is numeric.
    #[must_use]
    pub const fn as_number(&self) -> Option<f64> {
        match self {
            Self::Number(value) => Some(*value),
            _ => None,
        }
    }

    /// Returns the contained string when this value is textual.
    #[must_use]
    pub fn as_str(&self) -> Option<&str> {
        match self {
            Self::String(value) => Some(value.as_str()),
            _ => None,
        }
    }
}

/// One bounded DOM element description supplied by the engine.
///
/// This structure deliberately contains plain owned data instead of
/// `phantom-dom` types, preserving the dependency direction: the engine may
/// feed `phantom-js`, but `phantom-js` remains independent of the DOM crate.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BrowserDomElementSnapshot {
    node_id: u64,
    tag_name: String,
    attributes: BTreeMap<String, String>,
    text_content: String,
    parent_node_id: Option<u64>,
    connected: bool,
}

impl BrowserDomElementSnapshot {
    /// Creates one bounded element snapshot.
    ///
    /// # Errors
    ///
    /// Returns [`RuntimeError`] when tag/text/attribute budgets are exceeded.
    pub fn new(
        node_id: u64,
        tag_name: impl Into<String>,
        attributes: BTreeMap<String, String>,
        text_content: impl Into<String>,
    ) -> Result<Self, RuntimeError> {
        let tag_name = tag_name.into();
        let text_content = text_content.into();

        if tag_name.len() > MAX_HOST_TAG_BYTES
            || text_content.len() > MAX_HOST_TEXT_BYTES
            || attributes.len() > MAX_HOST_ATTRIBUTES_PER_ELEMENT
        {
            return Err(RuntimeError::new(
                RuntimeErrorKind::HostSnapshotLimitExceeded,
                0,
            ));
        }

        let attribute_bytes = attributes.iter().fold(0usize, |total, (name, value)| {
            total.saturating_add(name.len()).saturating_add(value.len())
        });
        if attribute_bytes > MAX_HOST_ATTRIBUTE_BYTES_PER_ELEMENT {
            return Err(RuntimeError::new(
                RuntimeErrorKind::HostSnapshotLimitExceeded,
                0,
            ));
        }

        Ok(Self {
            node_id,
            tag_name,
            attributes,
            text_content,
            parent_node_id: None,
            connected: true,
        })
    }

    /// Attaches the parent node identifier captured by the engine.
    #[must_use]
    pub const fn with_parent_node_id(mut self, parent_node_id: Option<u64>) -> Self {
        self.parent_node_id = parent_node_id;
        self
    }

    /// Marks whether this element is connected to the active document root.
    #[must_use]
    pub const fn with_connected(mut self, connected: bool) -> Self {
        self.connected = connected;
        self
    }

    /// Returns the parent node identifier, when present.
    #[must_use]
    pub const fn parent_node_id(&self) -> Option<u64> {
        self.parent_node_id
    }

    /// Returns whether this node is connected to the active document root.
    #[must_use]
    pub const fn connected(&self) -> bool {
        self.connected
    }

    /// Returns the stable engine DOM node identifier.
    #[must_use]
    pub const fn node_id(&self) -> u64 {
        self.node_id
    }

    /// Returns the normalized HTML tag name.
    #[must_use]
    pub fn tag_name(&self) -> &str {
        &self.tag_name
    }

    /// Returns the element attributes.
    #[must_use]
    pub const fn attributes(&self) -> &BTreeMap<String, String> {
        &self.attributes
    }

    /// Returns descendant text content captured by the engine.
    #[must_use]
    pub fn text_content(&self) -> &str {
        &self.text_content
    }
}

/// One bounded DOM mutation requested by JavaScript.
///
/// The engine is responsible for applying these commands to the real DOM and
/// rebuilding style/layout/paint snapshots.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BrowserDomMutation {
    /// Creates one detached element represented by a temporary runtime node id.
    CreateElement {
        /// Temporary runtime node identifier.
        node_id: u64,
        /// Normalized HTML tag name.
        tag_name: String,
    },
    /// Appends or moves one element below another.
    AppendChild {
        /// Parent engine/runtime node identifier.
        parent_id: u64,
        /// Child engine/runtime node identifier.
        child_id: u64,
    },
    /// Detaches one element from its current parent while preserving identity.
    DetachNode {
        /// Engine/runtime node identifier to detach.
        node_id: u64,
    },
    /// Replaces one element's descendant text.
    SetTextContent {
        /// Stable engine DOM node identifier.
        node_id: u64,
        /// Replacement text.
        value: String,
    },
    /// Sets or replaces one normalized element attribute.
    SetAttribute {
        /// Stable engine DOM node identifier.
        node_id: u64,
        /// Attribute name.
        name: String,
        /// Attribute value.
        value: String,
    },
    /// Removes one normalized element attribute.
    RemoveAttribute {
        /// Stable engine DOM node identifier.
        node_id: u64,
        /// Attribute name.
        name: String,
    },
}

/// Snapshot supplied by the browser/engine boundary to seed browser globals.
///
/// Browser Host Foundation stays snapshot-based: `phantom-js` does not depend
/// on `phantom-dom`, networking, or the native browser shell.
#[derive(Debug, Clone, PartialEq)]
pub struct BrowserHostSnapshot {
    url: String,
    title: String,
    inner_width: f64,
    inner_height: f64,
    device_pixel_ratio: f64,
    elements: Vec<BrowserDomElementSnapshot>,
}

impl BrowserHostSnapshot {
    /// Builds one bounded browser-host snapshot without DOM elements.
    ///
    /// # Errors
    ///
    /// Returns [`RuntimeError`] when URL/title budgets are exceeded or viewport
    /// numbers are not finite and non-negative.
    pub fn new(
        url: impl Into<String>,
        title: impl Into<String>,
        inner_width: f64,
        inner_height: f64,
        device_pixel_ratio: f64,
    ) -> Result<Self, RuntimeError> {
        let url = url.into();
        let title = title.into();

        if url.len() > MAX_HOST_URL_BYTES || title.len() > MAX_HOST_TITLE_BYTES {
            return Err(RuntimeError::new(
                RuntimeErrorKind::HostSnapshotLimitExceeded,
                0,
            ));
        }

        let valid_geometry = inner_width.is_finite()
            && inner_width >= 0.0
            && inner_height.is_finite()
            && inner_height >= 0.0
            && device_pixel_ratio.is_finite()
            && device_pixel_ratio > 0.0;

        if !valid_geometry {
            return Err(RuntimeError::new(RuntimeErrorKind::InvalidHostSnapshot, 0));
        }

        Ok(Self {
            url,
            title,
            inner_width,
            inner_height,
            device_pixel_ratio,
            elements: Vec::new(),
        })
    }

    /// Attaches bounded DOM element descriptions to this host snapshot.
    ///
    /// # Errors
    ///
    /// Returns [`RuntimeError`] when the DOM element count exceeds the browser
    /// host budget.
    pub fn with_dom_elements(
        mut self,
        elements: Vec<BrowserDomElementSnapshot>,
    ) -> Result<Self, RuntimeError> {
        if elements.len() > MAX_HOST_ELEMENTS {
            return Err(RuntimeError::new(
                RuntimeErrorKind::HostSnapshotLimitExceeded,
                0,
            ));
        }
        self.elements = elements;
        Ok(self)
    }

    /// Returns the committed document URL.
    #[must_use]
    pub fn url(&self) -> &str {
        &self.url
    }

    /// Returns the current document title.
    #[must_use]
    pub fn title(&self) -> &str {
        &self.title
    }

    /// Returns CSS viewport width in pixels.
    #[must_use]
    pub const fn inner_width(&self) -> f64 {
        self.inner_width
    }

    /// Returns CSS viewport height in pixels.
    #[must_use]
    pub const fn inner_height(&self) -> f64 {
        self.inner_height
    }

    /// Returns the device-pixel ratio exposed to script.
    #[must_use]
    pub const fn device_pixel_ratio(&self) -> f64 {
        self.device_pixel_ratio
    }

    /// Returns bounded DOM elements in stable engine order.
    #[must_use]
    pub fn dom_elements(&self) -> &[BrowserDomElementSnapshot] {
        &self.elements
    }
}

#[derive(Debug, Clone, Copy)]
struct BrowserHostHandles {
    window: usize,
    document: usize,
    location: usize,
    get_element_by_id: usize,
    query_selector: usize,
    query_selector_all: usize,
    get_elements_by_class_name: usize,
    get_elements_by_tag_name: usize,
    create_element: usize,
    set_timeout: usize,
    clear_timeout: usize,
    queue_microtask: usize,
    promise: usize,
    fetch: usize,
    // PHANTOM_2M16_WEB_GLOBALS_CORE_BUILTINS_I
    console: usize,
    math: usize,
    array: usize,
    number: usize,
    parse_int: usize,
    parse_float: usize,
    is_nan: usize,
    // PHANTOM_2M17_WEB_CORE_LIBRARY_II
    object: usize,
    json: usize,
    array_push: usize,
    array_pop: usize,
    array_join: usize,
    array_index_of: usize,
    array_includes: usize,
    array_slice: usize,
    string_trim: usize,
    string_to_lower_case: usize,
    string_to_upper_case: usize,
    string_includes: usize,
    string_starts_with: usize,
    string_ends_with: usize,
    string_index_of: usize,
    string_slice: usize,
    // PHANTOM_2M18_WEB_CORE_LIBRARY_III
    array_for_each: usize,
    array_map: usize,
    array_filter: usize,
    array_some: usize,
    array_every: usize,
    array_find: usize,
    array_find_index: usize,
    array_reduce: usize,
    string_split: usize,
    string_replace: usize,
    // PHANTOM_2M19_WEB_CORE_LIBRARY_FINAL_GATE
    array_at: usize,
    array_shift: usize,
    array_unshift: usize,
    array_concat: usize,
    array_reverse: usize,
    string_char_at: usize,
    string_substring: usize,
    string_repeat: usize,
    string_pad_start: usize,
    string_pad_end: usize,
    string_replace_all: usize,
}

#[derive(Debug, Clone)]
enum NativeFunctionKind {
    GetElementById,
    QuerySelector,
    QuerySelectorAll,
    GetElementsByClassName,
    GetElementsByTagName,
    CreateElement,
    SetTimeout,
    ClearTimeout,
    QueueMicrotask,
    PromiseResolve,
    PromiseReject,
    PromiseThen(usize),
    PromiseCatch(usize),
    ThenableResolve(usize, usize),
    ThenableReject(usize),
    // PHANTOM_2M10_DOM_EVENT_CONTROL_III
    EventPreventDefault(usize),
    EventStopPropagation(usize),
    EventStopImmediatePropagation(usize),
    Fetch,
    ResponseText(usize),
    // PHANTOM_2N18_RESPONSE_HEADERS_EXPOSURE_I
    ResponseHeaderGet(usize),
    ResponseHeaderHas(usize),
    // PHANTOM_2M16_WEB_GLOBALS_CORE_BUILTINS_I
    ConsoleNoop,
    MathAbs,
    MathFloor,
    MathCeil,
    MathRound,
    MathMin,
    MathMax,
    MathPow,
    MathSqrt,
    ArrayIsArray,
    NumberIsFinite,
    ParseInt,
    ParseFloat,
    GlobalIsNaN,
    // PHANTOM_2M17_WEB_CORE_LIBRARY_II
    ObjectKeys,
    ObjectValues,
    ObjectAssign,
    JsonParse,
    JsonStringify,
    ArrayPush,
    ArrayPop,
    ArrayJoin,
    ArrayIndexOf,
    ArrayIncludes,
    ArraySlice,
    StringTrim,
    StringToLowerCase,
    StringToUpperCase,
    StringIncludes,
    StringStartsWith,
    StringEndsWith,
    StringIndexOf,
    StringSlice,
    // PHANTOM_2M18_WEB_CORE_LIBRARY_III
    ObjectEntries,
    ObjectHasOwn,
    ArrayForEach,
    ArrayMap,
    ArrayFilter,
    ArraySome,
    ArrayEvery,
    ArrayFind,
    ArrayFindIndex,
    ArrayReduce,
    StringSplit,
    StringReplace,
    // PHANTOM_2M19_WEB_CORE_LIBRARY_FINAL_GATE
    ObjectFromEntries,
    MathTrunc,
    MathSign,
    ArrayOf,
    NumberIsInteger,
    NumberIsNaN,
    ArrayAt,
    ArrayShift,
    ArrayUnshift,
    ArrayConcat,
    ArrayReverse,
    StringCharAt,
    StringSubstring,
    StringRepeat,
    StringPadStart,
    StringPadEnd,
    StringReplaceAll,
}

// PHANTOM_2M21_DOM_SELECTOR_API_COMPATIBILITY_II
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum HostSelectorCombinator {
    Descendant,
    Child,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct HostSelectorAttribute {
    name: String,
    value: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct HostSelectorCompound {
    tag: Option<String>,
    id: Option<String>,
    classes: Vec<String>,
    attributes: Vec<HostSelectorAttribute>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct HostSelectorStep {
    compound: HostSelectorCompound,
    relation_to_previous: Option<HostSelectorCombinator>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct HostSelector {
    groups: Vec<Vec<HostSelectorStep>>,
}

/// Runtime failure category for Phantom JavaScript Runtime Core II.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RuntimeErrorKind {
    /// Source failed during the existing bounded lexer/parser stage.
    Parse(ParseErrorKind),
    /// Expression category is parsed but still outside Runtime Core II.
    UnsupportedExpression,
    /// Assignment target is outside the supported identifier/member subset.
    UnsupportedAssignmentTarget,
    /// Identifier lookup or assignment could not find a binding.
    ReferenceError,
    /// An immutable `const` binding was assigned.
    ConstAssignment,
    /// A lexical binding was declared twice in the same scope.
    DuplicateBinding,
    /// Runtime statement/expression work exceeded the execution budget.
    StepLimitExceeded,
    /// Lexical scope nesting exceeded the runtime scope budget.
    ScopeLimitExceeded,
    /// A scope exceeded the bounded binding count.
    BindingLimitExceeded,
    /// Numeric literal could not be converted safely.
    InvalidNumberLiteral,
    /// String literal could not be decoded by the current runtime subset.
    InvalidStringLiteral,
    /// Runtime attempted to propagate `return` without an executing function.
    ReturnOutsideFunction,
    /// Array heap count or array index would exceed the runtime budget.
    ArrayLimitExceeded,
    /// Object heap count exceeded the runtime budget.
    ObjectLimitExceeded,
    /// One object exceeded its property budget.
    PropertyLimitExceeded,
    /// Function heap count exceeded the runtime budget.
    FunctionLimitExceeded,
    /// Function arguments exceeded the per-call budget.
    ArgumentLimitExceeded,
    /// Nested calls exceeded the runtime call-depth budget.
    CallDepthExceeded,
    /// Retained source owned by function records exceeded its memory budget.
    RetainedSourceLimitExceeded,
    /// Member access targeted a value without members in Runtime Core II.
    InvalidMemberTarget,
    /// Computed property could not be represented by the bounded property model.
    InvalidPropertyKey,
    /// A call expression evaluated to a non-function value.
    CallNonFunction,
    /// Browser-host snapshot exceeded its bounded text budget.
    HostSnapshotLimitExceeded,
    /// Browser-host geometry was non-finite or otherwise invalid.
    InvalidHostSnapshot,
    /// Runtime native-function heap exceeded its deterministic budget.
    NativeFunctionLimitExceeded,
    /// JavaScript requested too many DOM mutations in one execution.
    DomMutationLimitExceeded,
    /// Browser-host selector is outside the bounded selector subset.
    UnsupportedHostSelector,
    /// `document.createElement` received an invalid bounded HTML tag name.
    InvalidHostTagName,
    /// A DOM append would create an invalid parent/child hierarchy.
    InvalidDomHierarchy,
    /// A DOMTokenList token is empty, oversized, or contains ASCII whitespace.
    InvalidDomToken,
    /// A CSSStyleDeclaration property name is empty, oversized, or outside the bounded name subset.
    InvalidCssPropertyName,
    /// Event type is missing, empty, or exceeds the bounded event-name budget.
    InvalidEventType,
    /// `addEventListener` received a non-callable listener.
    InvalidEventCallback,
    /// EventTarget listener storage exceeded the deterministic runtime budget.
    EventListenerLimitExceeded,
    /// Listener options exceed the bounded capture/once/passive foundation.
    UnsupportedEventListenerOptions,
    /// `dispatchEvent` received a value without a bounded textual `type`.
    InvalidEventObject,
    /// Nested synchronous event dispatch exceeded the deterministic depth budget.
    EventDispatchDepthExceeded,
    /// Event propagation ancestry exceeded the deterministic path budget or was cyclic.
    EventPropagationPathExceeded,
    /// The runtime exhausted its reserved temporary node-id namespace.
    VirtualNodeIdExhausted,
    /// Active one-shot timers exceeded the deterministic runtime budget.
    TimerLimitExceeded,
    /// `setTimeout` callback is outside the supported JavaScript-function subset.
    InvalidTimerCallback,
    /// Extra `setTimeout` callback arguments are deferred beyond Timers I.
    UnsupportedTimerArguments,
    /// One timer pump would execute too many ready tasks.
    TimerTaskLimitExceeded,
    /// Monotonic virtual timer clock overflowed.
    TimerClockOverflow,
    /// Timer identifier space was exhausted.
    TimerIdExhausted,
    /// Timer insertion-order sequence space was exhausted.
    TimerSequenceExhausted,
    /// Pending microtasks exceeded the deterministic runtime budget.
    MicrotaskLimitExceeded,
    /// One microtask checkpoint executed too many tasks.
    MicrotaskTaskLimitExceeded,
    /// `queueMicrotask` requires one callable callback.
    InvalidMicrotaskCallback,
    /// Promise heap exceeded its deterministic runtime budget.
    PromiseLimitExceeded,
    /// One pending promise accumulated too many reactions.
    PromiseReactionLimitExceeded,
    /// Too many fetch requests are pending host completion.
    FetchRequestLimitExceeded,
    /// `fetch` received a URL outside the bounded string-only foundation.
    InvalidFetchUrl,
    /// Fetch options/method/body are deferred beyond Fetch Foundation I.
    UnsupportedFetchOptions,
    /// Host attempted to complete a fetch request that is not pending.
    UnknownFetchRequest,
    /// Fetch response status is outside the bounded HTTP status range.
    InvalidFetchStatus,
    /// Fetch response URL/body exceeded the bounded resource budget.
    FetchResponseLimitExceeded,
    /// Fetch failure reason exceeded its bounded text budget.
    FetchFailureLimitExceeded,
    /// Response-object heap exceeded the Fetch Foundation I budget.
    FetchResponseCountExceeded,
    /// A core-library string result exceeded the deterministic runtime string budget.
    StringLimitExceeded,
    /// `JSON.parse` received malformed or unsupported bounded JSON input.
    InvalidJson,
    /// `Array.prototype.reduce` was called on an empty array without an initial value.
    ReduceEmptyArray,
}

/// Bounded execution error.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RuntimeError {
    kind: RuntimeErrorKind,
    offset: usize,
}

impl RuntimeError {
    const fn new(kind: RuntimeErrorKind, offset: usize) -> Self {
        Self { kind, offset }
    }

    /// Returns the runtime failure category.
    #[must_use]
    pub const fn kind(self) -> RuntimeErrorKind {
        self.kind
    }

    /// Returns the source byte offset associated with the failure.
    #[must_use]
    pub const fn offset(self) -> usize {
        self.offset
    }
}

impl fmt::Display for RuntimeError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "JavaScript runtime error {:?} at byte {}",
            self.kind, self.offset
        )
    }
}

impl Error for RuntimeError {}

#[derive(Debug, Clone)]
struct Binding {
    value: Value,
    mutable: bool,
}

#[derive(Debug, Clone)]
struct Scope {
    bindings: BTreeMap<String, Binding>,
    function_boundary: bool,
}

impl Scope {
    fn function_scope() -> Self {
        Self {
            bindings: BTreeMap::new(),
            function_boundary: true,
        }
    }

    fn block_scope() -> Self {
        Self {
            bindings: BTreeMap::new(),
            function_boundary: false,
        }
    }
}

#[derive(Debug, Clone)]
struct FunctionRecord {
    name: Option<String>,
    parameters: Vec<String>,
    body: Vec<Statement>,
    source: String,
    // PHANTOM_2M7_R1_FUNCTION_CONTEXT_RUNTIME_I
    // Non-global lexical scopes captured when the function value is created.
    // The global scope remains live and is supplied by the calling Runtime.
    captured_scopes: Vec<Scope>,
}

// PHANTOM_2M8_DOM_EVENT_TARGET_I
#[derive(Debug, Clone)]
struct EventListenerRecord {
    target_object: usize,
    event_type: String,
    callback: Value,
    capture: bool,
    // PHANTOM_2M10_DOM_EVENT_CONTROL_III
    // `once` is removed before invocation so re-entrant dispatch cannot run it twice.
    once: bool,
    // PHANTOM_2M11_DOM_EVENT_LISTENER_SEMANTICS_IV
    // Passive listeners may observe cancellation state but cannot cancel defaults.
    passive: bool,
    sequence: u64,
}

// PHANTOM_2M10_DOM_EVENT_CONTROL_III
#[derive(Debug, Clone)]
struct EventDispatchControl {
    event_object: usize,
    propagation_stopped: bool,
    immediate_stopped: bool,
    default_prevented: bool,
    cancelable: bool,
    // PHANTOM_2M11_DOM_EVENT_LISTENER_SEMANTICS_IV
    in_passive_listener: bool,
}

#[derive(Debug, Clone)]
struct TimerRecord {
    id: u32,
    due_at_ms: u64,
    sequence: u64,
    callback: usize,
}

#[derive(Debug, Clone)]
enum PromiseState {
    Pending,
    Fulfilled(Value),
    Rejected(Value),
}

#[derive(Debug, Clone)]
struct PromiseReaction {
    on_fulfilled: Option<Value>,
    on_rejected: Option<Value>,
    child: usize,
}

#[derive(Debug, Clone)]
struct PromiseRecord {
    state: PromiseState,
    reactions: Vec<PromiseReaction>,
}

#[derive(Debug, Clone)]
enum Microtask {
    Call {
        callback: Value,
        arguments: Vec<Value>,
    },
    // PHANTOM_2M4_R1_THENABLE_JOB_ISOLATION_I
    ThenableJob {
        callback: Value,
        arguments: Vec<Value>,
        promise_id: usize,
    },
    PromiseReaction {
        state: PromiseState,
        reaction: PromiseReaction,
    },
}

// PHANTOM_2N16_FETCH_REQUEST_INIT_HOST_BOUNDARY_I
/// Credentials mode requested by the bounded JavaScript `fetch` bridge.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FetchCredentialsMode {
    /// Never attach ambient credentials.
    Omit,
    /// Attach credentials only for same-origin requests.
    SameOrigin,
    /// Allow credentials according to the host/network policy.
    Include,
}

/// Structured, bounded fetch request emitted to the browser host.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FetchHostRequest {
    id: u64,
    url: String,
    method: String,
    headers: Vec<(String, String)>,
    body: Option<String>,
    credentials: FetchCredentialsMode,
}

impl FetchHostRequest {
    /// Host request identifier paired with completion APIs.
    #[must_use]
    pub const fn id(&self) -> u64 {
        self.id
    }

    /// Raw URL supplied by JavaScript.
    #[must_use]
    pub fn url(&self) -> &str {
        &self.url
    }

    /// Canonical uppercase method token.
    #[must_use]
    pub fn method(&self) -> &str {
        &self.method
    }

    /// Author headers captured from the bounded RequestInit object.
    #[must_use]
    pub fn headers(&self) -> &[(String, String)] {
        &self.headers
    }

    /// Optional textual request body.
    #[must_use]
    pub fn body(&self) -> Option<&str> {
        self.body.as_deref()
    }

    /// Requested credentials mode.
    #[must_use]
    pub const fn credentials(&self) -> FetchCredentialsMode {
        self.credentials
    }
}

// PHANTOM_2N18_RESPONSE_HEADERS_EXPOSURE_I
fn validate_response_headers_2n18(
    headers: &[(String, String)],
) -> Result<BTreeMap<String, String>, RuntimeError> {
    const NAMES: [&str; 7] = [
        "cache-control",
        "content-language",
        "content-length",
        "content-type",
        "expires",
        "last-modified",
        "pragma",
    ];
    if headers.len() > NAMES.len() {
        return Err(RuntimeError::new(
            RuntimeErrorKind::FetchResponseLimitExceeded,
            0,
        ));
    }
    let mut values = BTreeMap::new();
    for (name, value) in headers {
        let name = name.to_ascii_lowercase();
        if !NAMES.contains(&name.as_str())
            || value.len() > 8 * 1024
            || value.bytes().any(|byte| byte == b'\r' || byte == b'\n')
            || values.insert(name, value.clone()).is_some()
        {
            return Err(RuntimeError::new(
                RuntimeErrorKind::FetchResponseLimitExceeded,
                0,
            ));
        }
    }
    Ok(values)
}

type FetchRequestRecord = FetchHostRequest;

#[derive(Debug, Clone)]
struct FetchResponseRecord {
    body: String,
    body_used: bool,
    headers: BTreeMap<String, String>,
}

#[derive(Debug, Clone)]
struct FetchRequestInit {
    method: String,
    headers: Vec<(String, String)>,
    body: Option<String>,
    credentials: FetchCredentialsMode,
}

impl Default for FetchRequestInit {
    fn default() -> Self {
        Self {
            method: "GET".to_owned(),
            headers: Vec::new(),
            body: None,
            credentials: FetchCredentialsMode::SameOrigin,
        }
    }
}

#[derive(Debug, Clone)]
enum Completion {
    Normal(Value),
    Return(Value),
}

/// Bounded JavaScript runtime state.
///
/// One [`Runtime`] preserves global bindings and heap identities across
/// successive [`Runtime::execute`] calls. Each execution receives a fresh step
/// budget. Browser-host bindings remain bounded and dependency-inverted.
/// Timers I, Microtask/Promise Foundation I, Fetch Foundation I, and bounded
/// element EventTarget dispatch are deterministic and host governed. Networking
/// itself stays outside this crate: fetch requests cross an explicit bounded host
/// queue. Bounded DOM capture/target/bubble propagation, cancellation controls,
/// one-shot/passive listeners, listener objects, dispatch-time listener mutation,
/// `Element.classList`, a bounded live `Element.style` / CSSStyleDeclaration subset,
/// `dataset`, bounded HTMLElement string/boolean attribute reflection, compound/list/
/// descendant/child DOM selector APIs, web globals, and a bounded core-library slice
/// (`Object`, `JSON`, common Array/String methods,
/// `console`, `Math`, numeric parsing/checks) are supported. Element tree navigation
/// exposes bounded snapshot `children`, parent/element-sibling traversal, and live
/// mutation coherence for append/remove operations inside one execution.
/// Host-originated trusted click dispatch is supported while native default actions,
/// wall-clock integration, and automatic page-script execution remain host-owned.
#[derive(Debug, Clone)]
pub struct Runtime {
    scopes: Vec<Scope>,
    arrays: Vec<Vec<Value>>,
    objects: Vec<BTreeMap<String, Value>>,
    functions: Vec<FunctionRecord>,
    native_functions: Vec<NativeFunctionKind>,
    retained_function_source_bytes: usize,
    browser_host: Option<BrowserHostHandles>,
    host_elements: Vec<BrowserDomElementSnapshot>,
    host_element_objects: BTreeMap<u64, usize>,
    host_object_nodes: BTreeMap<usize, u64>,
    // PHANTOM_2M13_CSS_STYLE_DECLARATION_I
    // Style objects keep stable identity while resolving their live element through
    // the existing element-object -> NodeId map.
    host_style_object_elements: BTreeMap<usize, usize>,
    // PHANTOM_2M14_DOM_DATASET_ATTRIBUTE_REFLECTION_I
    // DOMStringMap objects are stable wrappers over each element's live `data-*` attributes.
    host_dataset_object_elements: BTreeMap<usize, usize>,
    dom_mutations: Vec<BrowserDomMutation>,
    next_virtual_node_serial: u64,
    event_listeners: Vec<EventListenerRecord>,
    next_event_listener_sequence: u64,
    event_dispatch_depth: usize,
    // PHANTOM_2M10_DOM_EVENT_CONTROL_III
    event_dispatch_controls: Vec<EventDispatchControl>,
    timers: Vec<TimerRecord>,
    current_time_ms: u64,
    next_timer_id: u32,
    next_timer_sequence: u64,
    microtasks: VecDeque<Microtask>,
    promises: BTreeMap<usize, PromiseRecord>,
    fetch_requests: VecDeque<FetchRequestRecord>,
    fetch_promises: BTreeMap<u64, usize>,
    fetch_responses: BTreeMap<usize, FetchResponseRecord>,
    next_fetch_id: u64,
    call_depth: usize,
    steps: usize,
}

impl Default for Runtime {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Debug, Clone, Copy)]
enum StringSearchKind {
    Includes,
    StartsWith,
    EndsWith,
    IndexOf,
}

impl Runtime {
    /// Creates an empty runtime with one global environment and empty heaps.
    #[must_use]
    pub fn new() -> Self {
        Self {
            scopes: vec![Scope::function_scope()],
            arrays: Vec::new(),
            objects: Vec::new(),
            functions: Vec::new(),
            native_functions: Vec::new(),
            retained_function_source_bytes: 0,
            browser_host: None,
            host_elements: Vec::new(),
            host_element_objects: BTreeMap::new(),
            host_object_nodes: BTreeMap::new(),
            host_style_object_elements: BTreeMap::new(),
            host_dataset_object_elements: BTreeMap::new(),
            dom_mutations: Vec::new(),
            next_virtual_node_serial: 0,
            event_listeners: Vec::new(),
            next_event_listener_sequence: 0,
            event_dispatch_depth: 0,
            event_dispatch_controls: Vec::new(),
            timers: Vec::new(),
            current_time_ms: 0,
            next_timer_id: 1,
            next_timer_sequence: 0,
            microtasks: VecDeque::new(),
            promises: BTreeMap::new(),
            fetch_requests: VecDeque::new(),
            fetch_promises: BTreeMap::new(),
            fetch_responses: BTreeMap::new(),
            next_fetch_id: 1,
            call_depth: 0,
            steps: 0,
        }
    }

    /// Executes one bounded Foundation 9 program.
    ///
    /// The returned value is the last normal statement/expression value, or
    /// [`Value::Undefined`] for an empty program.
    ///
    /// # Errors
    ///
    /// Returns [`RuntimeError`] for parse failures, unsupported host-context
    /// constructs, invalid references/member operations, or exceeded budgets.
    pub fn execute(&mut self, source: &str) -> Result<Value, RuntimeError> {
        let program = parse_program(source).map_err(|error| {
            RuntimeError::new(RuntimeErrorKind::Parse(error.kind()), error.offset())
        })?;

        self.steps = 0;
        self.dom_mutations.clear();
        let value = match self.execute_program(source, &program)? {
            Completion::Normal(value) => value,
            Completion::Return(_) => {
                return Err(RuntimeError::new(
                    RuntimeErrorKind::ReturnOutsideFunction,
                    program.span().start(),
                ));
            }
        };

        self.perform_microtask_checkpoint()?;
        Ok(value)
    }

    /// Returns one global binding value without mutating runtime state.
    #[must_use]
    pub fn global(&self, name: &str) -> Option<&Value> {
        self.scopes
            .first()
            .and_then(|scope| scope.bindings.get(name))
            .map(|binding| &binding.value)
    }

    /// Returns the monotonic virtual clock used by the timer queue.
    #[must_use]
    pub const fn current_time_ms(&self) -> u64 {
        self.current_time_ms
    }

    /// Returns the number of active one-shot timers.
    #[must_use]
    pub fn pending_timer_count(&self) -> usize {
        self.timers.len()
    }

    /// Returns the number of microtasks waiting for the next checkpoint.
    #[must_use]
    pub fn pending_microtask_count(&self) -> usize {
        self.microtasks.len()
    }

    /// Returns the number of fetch requests awaiting host completion.
    #[must_use]
    pub fn pending_fetch_count(&self) -> usize {
        self.fetch_promises.len()
    }

    // PHANTOM_2N16_FETCH_REQUEST_INIT_HOST_BOUNDARY_I
    /// Drains structured fetch requests including bounded RequestInit metadata.
    ///
    /// URL resolution, forbidden-header enforcement, CORS/preflight, credential
    /// attachment and actual I/O remain browser/network-host responsibilities.
    pub fn drain_fetch_host_requests(&mut self) -> Vec<FetchHostRequest> {
        self.fetch_requests.drain(..).collect()
    }

    /// Drains newly issued fetch requests for the browser/network host.
    ///
    /// Each tuple is `(request_id, raw_url)`. URL resolution, scheme policy,
    /// same-origin/CORS policy, redirects, cookies, TLS, cache behavior, and
    /// actual I/O remain outside `phantom-js`.
    pub fn drain_fetch_requests(&mut self) -> Vec<(u64, String)> {
        self.fetch_requests
            .drain(..)
            .map(|request| (request.id, request.url))
            .collect()
    }

    /// Completes one pending fetch with a bounded successful HTTP response.
    ///
    /// The completion is one host task: resolving the fetch promise immediately
    /// triggers the associated microtask checkpoint. Runtime state is staged so
    /// callback failure leaves the pre-completion state intact.
    ///
    /// # Errors
    ///
    /// Returns [`RuntimeError`] for unknown requests, invalid response metadata,
    /// budget exhaustion, or reaction callback failure.
    pub fn complete_fetch_success(
        &mut self,
        request_id: u64,
        status: u16,
        response_url: impl Into<String>,
        body: impl Into<String>,
    ) -> Result<(), RuntimeError> {
        let mut staged = self.clone();
        staged.complete_fetch_success_inner(
            request_id,
            status,
            response_url.into(),
            body.into(),
        )?;
        *self = staged;
        Ok(())
    }

    // PHANTOM_2N18_RESPONSE_HEADERS_EXPOSURE_I
    /// Completes fetch with only network-admitted, CORS-safelisted headers.
    ///
    /// The host must pass no raw or forbidden response headers. This runtime
    /// independently checks the seven-name allowlist and bounded value sizes.
    ///
    /// # Errors
    ///
    /// Rejects unknown fetches, invalid metadata, or header budget violations.
    pub fn complete_fetch_success_with_headers(
        &mut self,
        request_id: u64,
        status: u16,
        response_url: impl Into<String>,
        body: impl Into<String>,
        headers: &[(String, String)],
    ) -> Result<(), RuntimeError> {
        let admitted = validate_response_headers_2n18(headers)?;
        let mut staged = self.clone();
        staged.complete_fetch_success_inner_with_headers(
            request_id,
            status,
            response_url.into(),
            body.into(),
            admitted,
        )?;
        *self = staged;
        Ok(())
    }

    /// Completes one pending fetch as a bounded network failure.
    ///
    /// The fetch promise is rejected with the supplied textual reason and a
    /// microtask checkpoint is performed transactionally.
    ///
    /// # Errors
    ///
    /// Returns [`RuntimeError`] for unknown requests, oversized failure text,
    /// or reaction callback failure.
    pub fn complete_fetch_failure(
        &mut self,
        request_id: u64,
        reason: impl Into<String>,
    ) -> Result<(), RuntimeError> {
        let mut staged = self.clone();
        staged.complete_fetch_failure_inner(request_id, reason.into())?;
        *self = staged;
        Ok(())
    }

    /// Advances the bounded virtual timer clock and executes ready timer tasks.
    ///
    /// Timer execution is transactional at the runtime level: if one callback
    /// fails or the per-pump task budget is exceeded, timer/global/DOM-host
    /// state is left exactly as it was before this call.
    ///
    /// Timers scheduled for the same deadline execute in registration order.
    /// A callback may schedule additional zero-delay timers; they participate
    /// in the same pump while the task budget permits.
    ///
    /// # Errors
    ///
    /// Returns [`RuntimeError`] for clock overflow, callback execution failure,
    /// or timer-task budget exhaustion.
    pub fn advance_time(&mut self, elapsed_ms: u64) -> Result<usize, RuntimeError> {
        let mut staged = self.clone();
        let fired = staged.advance_time_inner(elapsed_ms)?;
        *self = staged;
        Ok(fired)
    }

    // PHANTOM_2M24_NATIVE_INTERACTION_JAVASCRIPT_EVENTS_I
    /// Dispatches one host-originated trusted click through the existing
    /// bounded EventTarget capture/target/bubble pipeline.
    ///
    /// The returned boolean is `true` when the browser may continue with
    /// the native default action and `false` when JavaScript called
    /// `preventDefault()` on the cancelable click. DOM mutations and
    /// microtasks produced by listeners remain part of the same host task.
    ///
    /// # Errors
    ///
    /// Returns [`RuntimeError`] when the target does not exist, event
    /// dispatch fails, or a listener/microtask exceeds a runtime budget.
    pub fn dispatch_trusted_click(&mut self, node_id: u64) -> Result<bool, RuntimeError> {
        let mut staged = self.clone();
        staged.steps = 0;
        staged.dom_mutations.clear();

        let target = staged.materialize_host_element(node_id, 0)?;
        let Value::Object(object_id) = target else {
            return Err(RuntimeError::new(RuntimeErrorKind::InvalidMemberTarget, 0));
        };

        let disabled = staged.host_element(node_id).is_some_and(|element| {
            element.attributes.contains_key("disabled")
                && matches!(
                    element.tag_name.as_str(),
                    "button" | "input" | "select" | "textarea" | "option" | "optgroup" | "fieldset"
                )
        });
        if disabled {
            *self = staged;
            return Ok(false);
        }

        let event = staged.allocate_object(
            BTreeMap::from([
                ("type".to_owned(), Value::String("click".to_owned())),
                ("bubbles".to_owned(), Value::Boolean(true)),
                ("cancelable".to_owned(), Value::Boolean(true)),
            ]),
            0,
        )?;
        let Value::Object(event_id) = event else {
            return Err(RuntimeError::new(RuntimeErrorKind::InvalidEventObject, 0));
        };

        let default_allowed =
            staged.dispatch_event_object(object_id, node_id, event_id, 0, true)?;
        staged.perform_microtask_checkpoint()?;

        *self = staged;
        Ok(default_allowed)
    }

    /// Installs or refreshes the snapshot-backed browser globals.
    ///
    /// Stable `window`, `document`, `location`, and already-materialized
    /// element identities survive refreshes for nodes that still exist.
    ///
    /// # Errors
    ///
    /// Returns [`RuntimeError`] when browser-host object/function/binding
    /// budgets would be exceeded.
    pub fn install_browser_host(
        &mut self,
        snapshot: &BrowserHostSnapshot,
    ) -> Result<(), RuntimeError> {
        let handles = match self.browser_host {
            Some(handles) => handles,
            None => {
                let location = object_id(self.allocate_object(BTreeMap::new(), 0)?, 0)?;
                let document = object_id(self.allocate_object(BTreeMap::new(), 0)?, 0)?;
                let window = object_id(self.allocate_object(BTreeMap::new(), 0)?, 0)?;
                let get_element_by_id =
                    self.allocate_native_function(NativeFunctionKind::GetElementById, 0)?;
                let query_selector =
                    self.allocate_native_function(NativeFunctionKind::QuerySelector, 0)?;
                let query_selector_all =
                    self.allocate_native_function(NativeFunctionKind::QuerySelectorAll, 0)?;
                let get_elements_by_class_name =
                    self.allocate_native_function(NativeFunctionKind::GetElementsByClassName, 0)?;
                let get_elements_by_tag_name =
                    self.allocate_native_function(NativeFunctionKind::GetElementsByTagName, 0)?;
                let create_element =
                    self.allocate_native_function(NativeFunctionKind::CreateElement, 0)?;
                let set_timeout =
                    self.allocate_native_function(NativeFunctionKind::SetTimeout, 0)?;
                let clear_timeout =
                    self.allocate_native_function(NativeFunctionKind::ClearTimeout, 0)?;
                let queue_microtask =
                    self.allocate_native_function(NativeFunctionKind::QueueMicrotask, 0)?;
                let fetch = self.allocate_native_function(NativeFunctionKind::Fetch, 0)?;
                let promise = object_id(self.allocate_object(BTreeMap::new(), 0)?, 0)?;
                let promise_resolve =
                    self.allocate_native_function(NativeFunctionKind::PromiseResolve, 0)?;
                let promise_reject =
                    self.allocate_native_function(NativeFunctionKind::PromiseReject, 0)?;

                // PHANTOM_2M16_WEB_GLOBALS_CORE_BUILTINS_I
                // Browser-like globals are allocated once per runtime and keep stable identity
                // across host refreshes. This is a bounded compatibility slice, not a prototype
                // implementation: the exposed methods are own properties of stable host objects.
                let console = object_id(self.allocate_object(BTreeMap::new(), 0)?, 0)?;
                let console_log =
                    self.allocate_native_function(NativeFunctionKind::ConsoleNoop, 0)?;
                let console_info =
                    self.allocate_native_function(NativeFunctionKind::ConsoleNoop, 0)?;
                let console_warn =
                    self.allocate_native_function(NativeFunctionKind::ConsoleNoop, 0)?;
                let console_error =
                    self.allocate_native_function(NativeFunctionKind::ConsoleNoop, 0)?;
                self.replace_host_object(
                    console,
                    BTreeMap::from([
                        ("log".to_owned(), Value::NativeFunction(console_log)),
                        ("info".to_owned(), Value::NativeFunction(console_info)),
                        ("warn".to_owned(), Value::NativeFunction(console_warn)),
                        ("error".to_owned(), Value::NativeFunction(console_error)),
                    ]),
                )?;

                let math = object_id(self.allocate_object(BTreeMap::new(), 0)?, 0)?;
                let math_abs = self.allocate_native_function(NativeFunctionKind::MathAbs, 0)?;
                let math_floor = self.allocate_native_function(NativeFunctionKind::MathFloor, 0)?;
                let math_ceil = self.allocate_native_function(NativeFunctionKind::MathCeil, 0)?;
                let math_round = self.allocate_native_function(NativeFunctionKind::MathRound, 0)?;
                let math_min = self.allocate_native_function(NativeFunctionKind::MathMin, 0)?;
                let math_max = self.allocate_native_function(NativeFunctionKind::MathMax, 0)?;
                let math_pow = self.allocate_native_function(NativeFunctionKind::MathPow, 0)?;
                let math_sqrt = self.allocate_native_function(NativeFunctionKind::MathSqrt, 0)?;
                // PHANTOM_2M19_WEB_CORE_LIBRARY_FINAL_GATE
                let math_trunc = self.allocate_native_function(NativeFunctionKind::MathTrunc, 0)?;
                let math_sign = self.allocate_native_function(NativeFunctionKind::MathSign, 0)?;
                self.replace_host_object(
                    math,
                    BTreeMap::from([
                        ("E".to_owned(), Value::Number(std::f64::consts::E)),
                        ("PI".to_owned(), Value::Number(std::f64::consts::PI)),
                        ("abs".to_owned(), Value::NativeFunction(math_abs)),
                        ("ceil".to_owned(), Value::NativeFunction(math_ceil)),
                        ("floor".to_owned(), Value::NativeFunction(math_floor)),
                        ("max".to_owned(), Value::NativeFunction(math_max)),
                        ("min".to_owned(), Value::NativeFunction(math_min)),
                        ("pow".to_owned(), Value::NativeFunction(math_pow)),
                        ("round".to_owned(), Value::NativeFunction(math_round)),
                        ("sqrt".to_owned(), Value::NativeFunction(math_sqrt)),
                        ("trunc".to_owned(), Value::NativeFunction(math_trunc)),
                        ("sign".to_owned(), Value::NativeFunction(math_sign)),
                    ]),
                )?;

                let array = object_id(self.allocate_object(BTreeMap::new(), 0)?, 0)?;
                let array_is_array =
                    self.allocate_native_function(NativeFunctionKind::ArrayIsArray, 0)?;
                // PHANTOM_2M19_WEB_CORE_LIBRARY_FINAL_GATE
                let array_of = self.allocate_native_function(NativeFunctionKind::ArrayOf, 0)?;
                self.replace_host_object(
                    array,
                    BTreeMap::from([
                        ("isArray".to_owned(), Value::NativeFunction(array_is_array)),
                        ("of".to_owned(), Value::NativeFunction(array_of)),
                    ]),
                )?;

                let number = object_id(self.allocate_object(BTreeMap::new(), 0)?, 0)?;
                let number_is_finite =
                    self.allocate_native_function(NativeFunctionKind::NumberIsFinite, 0)?;
                // PHANTOM_2M19_WEB_CORE_LIBRARY_FINAL_GATE
                let number_is_integer =
                    self.allocate_native_function(NativeFunctionKind::NumberIsInteger, 0)?;
                let number_is_nan =
                    self.allocate_native_function(NativeFunctionKind::NumberIsNaN, 0)?;
                self.replace_host_object(
                    number,
                    BTreeMap::from([
                        ("NaN".to_owned(), Value::Number(f64::NAN)),
                        ("POSITIVE_INFINITY".to_owned(), Value::Number(f64::INFINITY)),
                        (
                            "NEGATIVE_INFINITY".to_owned(),
                            Value::Number(f64::NEG_INFINITY),
                        ),
                        (
                            "isFinite".to_owned(),
                            Value::NativeFunction(number_is_finite),
                        ),
                        (
                            "isInteger".to_owned(),
                            Value::NativeFunction(number_is_integer),
                        ),
                        ("isNaN".to_owned(), Value::NativeFunction(number_is_nan)),
                    ]),
                )?;

                let parse_int = self.allocate_native_function(NativeFunctionKind::ParseInt, 0)?;
                let parse_float =
                    self.allocate_native_function(NativeFunctionKind::ParseFloat, 0)?;
                let is_nan = self.allocate_native_function(NativeFunctionKind::GlobalIsNaN, 0)?;

                // PHANTOM_2M17_WEB_CORE_LIBRARY_II
                let object = object_id(self.allocate_object(BTreeMap::new(), 0)?, 0)?;
                let object_keys =
                    self.allocate_native_function(NativeFunctionKind::ObjectKeys, 0)?;
                let object_values =
                    self.allocate_native_function(NativeFunctionKind::ObjectValues, 0)?;
                let object_assign =
                    self.allocate_native_function(NativeFunctionKind::ObjectAssign, 0)?;
                // PHANTOM_2M18_WEB_CORE_LIBRARY_III
                let object_entries =
                    self.allocate_native_function(NativeFunctionKind::ObjectEntries, 0)?;
                let object_has_own =
                    self.allocate_native_function(NativeFunctionKind::ObjectHasOwn, 0)?;
                // PHANTOM_2M19_WEB_CORE_LIBRARY_FINAL_GATE
                let object_from_entries =
                    self.allocate_native_function(NativeFunctionKind::ObjectFromEntries, 0)?;
                self.replace_host_object(
                    object,
                    BTreeMap::from([
                        ("assign".to_owned(), Value::NativeFunction(object_assign)),
                        ("entries".to_owned(), Value::NativeFunction(object_entries)),
                        (
                            "fromEntries".to_owned(),
                            Value::NativeFunction(object_from_entries),
                        ),
                        ("hasOwn".to_owned(), Value::NativeFunction(object_has_own)),
                        ("keys".to_owned(), Value::NativeFunction(object_keys)),
                        ("values".to_owned(), Value::NativeFunction(object_values)),
                    ]),
                )?;

                let json = object_id(self.allocate_object(BTreeMap::new(), 0)?, 0)?;
                let json_parse = self.allocate_native_function(NativeFunctionKind::JsonParse, 0)?;
                let json_stringify =
                    self.allocate_native_function(NativeFunctionKind::JsonStringify, 0)?;
                self.replace_host_object(
                    json,
                    BTreeMap::from([
                        ("parse".to_owned(), Value::NativeFunction(json_parse)),
                        (
                            "stringify".to_owned(),
                            Value::NativeFunction(json_stringify),
                        ),
                    ]),
                )?;

                let array_push = self.allocate_native_function(NativeFunctionKind::ArrayPush, 0)?;
                let array_pop = self.allocate_native_function(NativeFunctionKind::ArrayPop, 0)?;
                let array_join = self.allocate_native_function(NativeFunctionKind::ArrayJoin, 0)?;
                let array_index_of =
                    self.allocate_native_function(NativeFunctionKind::ArrayIndexOf, 0)?;
                let array_includes =
                    self.allocate_native_function(NativeFunctionKind::ArrayIncludes, 0)?;
                let array_slice =
                    self.allocate_native_function(NativeFunctionKind::ArraySlice, 0)?;
                let string_trim =
                    self.allocate_native_function(NativeFunctionKind::StringTrim, 0)?;
                let string_to_lower_case =
                    self.allocate_native_function(NativeFunctionKind::StringToLowerCase, 0)?;
                let string_to_upper_case =
                    self.allocate_native_function(NativeFunctionKind::StringToUpperCase, 0)?;
                let string_includes =
                    self.allocate_native_function(NativeFunctionKind::StringIncludes, 0)?;
                let string_starts_with =
                    self.allocate_native_function(NativeFunctionKind::StringStartsWith, 0)?;
                let string_ends_with =
                    self.allocate_native_function(NativeFunctionKind::StringEndsWith, 0)?;
                let string_index_of =
                    self.allocate_native_function(NativeFunctionKind::StringIndexOf, 0)?;
                let string_slice =
                    self.allocate_native_function(NativeFunctionKind::StringSlice, 0)?;

                // PHANTOM_2M18_WEB_CORE_LIBRARY_III
                let array_for_each =
                    self.allocate_native_function(NativeFunctionKind::ArrayForEach, 0)?;
                let array_map = self.allocate_native_function(NativeFunctionKind::ArrayMap, 0)?;
                let array_filter =
                    self.allocate_native_function(NativeFunctionKind::ArrayFilter, 0)?;
                let array_some = self.allocate_native_function(NativeFunctionKind::ArraySome, 0)?;
                let array_every =
                    self.allocate_native_function(NativeFunctionKind::ArrayEvery, 0)?;
                let array_find = self.allocate_native_function(NativeFunctionKind::ArrayFind, 0)?;
                let array_find_index =
                    self.allocate_native_function(NativeFunctionKind::ArrayFindIndex, 0)?;
                let array_reduce =
                    self.allocate_native_function(NativeFunctionKind::ArrayReduce, 0)?;
                let string_split =
                    self.allocate_native_function(NativeFunctionKind::StringSplit, 0)?;
                let string_replace =
                    self.allocate_native_function(NativeFunctionKind::StringReplace, 0)?;

                // PHANTOM_2M19_WEB_CORE_LIBRARY_FINAL_GATE
                let array_at = self.allocate_native_function(NativeFunctionKind::ArrayAt, 0)?;
                let array_shift =
                    self.allocate_native_function(NativeFunctionKind::ArrayShift, 0)?;
                let array_unshift =
                    self.allocate_native_function(NativeFunctionKind::ArrayUnshift, 0)?;
                let array_concat =
                    self.allocate_native_function(NativeFunctionKind::ArrayConcat, 0)?;
                let array_reverse =
                    self.allocate_native_function(NativeFunctionKind::ArrayReverse, 0)?;
                let string_char_at =
                    self.allocate_native_function(NativeFunctionKind::StringCharAt, 0)?;
                let string_substring =
                    self.allocate_native_function(NativeFunctionKind::StringSubstring, 0)?;
                let string_repeat =
                    self.allocate_native_function(NativeFunctionKind::StringRepeat, 0)?;
                let string_pad_start =
                    self.allocate_native_function(NativeFunctionKind::StringPadStart, 0)?;
                let string_pad_end =
                    self.allocate_native_function(NativeFunctionKind::StringPadEnd, 0)?;
                let string_replace_all =
                    self.allocate_native_function(NativeFunctionKind::StringReplaceAll, 0)?;

                self.replace_host_object(
                    promise,
                    BTreeMap::from([
                        ("resolve".to_owned(), Value::NativeFunction(promise_resolve)),
                        ("reject".to_owned(), Value::NativeFunction(promise_reject)),
                    ]),
                )?;

                let handles = BrowserHostHandles {
                    window,
                    document,
                    location,
                    get_element_by_id,
                    query_selector,
                    query_selector_all,
                    get_elements_by_class_name,
                    get_elements_by_tag_name,
                    create_element,
                    set_timeout,
                    clear_timeout,
                    queue_microtask,
                    promise,
                    fetch,
                    console,
                    math,
                    array,
                    number,
                    parse_int,
                    parse_float,
                    is_nan,
                    object,
                    json,
                    array_push,
                    array_pop,
                    array_join,
                    array_index_of,
                    array_includes,
                    array_slice,
                    string_trim,
                    string_to_lower_case,
                    string_to_upper_case,
                    string_includes,
                    string_starts_with,
                    string_ends_with,
                    string_index_of,
                    string_slice,
                    array_for_each,
                    array_map,
                    array_filter,
                    array_some,
                    array_every,
                    array_find,
                    array_find_index,
                    array_reduce,
                    string_split,
                    string_replace,
                    array_at,
                    array_shift,
                    array_unshift,
                    array_concat,
                    array_reverse,
                    string_char_at,
                    string_substring,
                    string_repeat,
                    string_pad_start,
                    string_pad_end,
                    string_replace_all,
                };
                self.browser_host = Some(handles);
                handles
            }
        };

        self.host_elements = snapshot.elements.clone();

        let document_element_id = self.host_elements.iter().find_map(|element| {
            (element.connected && element.tag_name.eq_ignore_ascii_case("html"))
                .then_some(element.node_id)
        });
        let body_id = self.host_elements.iter().find_map(|element| {
            (element.connected && element.tag_name.eq_ignore_ascii_case("body"))
                .then_some(element.node_id)
        });
        let document_element = match document_element_id {
            Some(node_id) => self.materialize_host_element(node_id, 0)?,
            None => Value::Null,
        };
        let body = match body_id {
            Some(node_id) => self.materialize_host_element(node_id, 0)?,
            None => Value::Null,
        };

        self.replace_host_object(
            handles.location,
            BTreeMap::from([("href".to_owned(), Value::String(snapshot.url.clone()))]),
        )?;

        self.replace_host_object(
            handles.document,
            BTreeMap::from([
                ("URL".to_owned(), Value::String(snapshot.url.clone())),
                ("title".to_owned(), Value::String(snapshot.title.clone())),
                (
                    "readyState".to_owned(),
                    Value::String("complete".to_owned()),
                ),
                ("location".to_owned(), Value::Object(handles.location)),
                (
                    "getElementById".to_owned(),
                    Value::NativeFunction(handles.get_element_by_id),
                ),
                (
                    "querySelector".to_owned(),
                    Value::NativeFunction(handles.query_selector),
                ),
                (
                    "querySelectorAll".to_owned(),
                    Value::NativeFunction(handles.query_selector_all),
                ),
                (
                    "getElementsByClassName".to_owned(),
                    Value::NativeFunction(handles.get_elements_by_class_name),
                ),
                (
                    "getElementsByTagName".to_owned(),
                    Value::NativeFunction(handles.get_elements_by_tag_name),
                ),
                ("documentElement".to_owned(), document_element),
                ("body".to_owned(), body),
                (
                    "createElement".to_owned(),
                    Value::NativeFunction(handles.create_element),
                ),
            ]),
        )?;

        self.replace_host_object(
            handles.window,
            BTreeMap::from([
                ("window".to_owned(), Value::Object(handles.window)),
                ("self".to_owned(), Value::Object(handles.window)),
                ("document".to_owned(), Value::Object(handles.document)),
                ("location".to_owned(), Value::Object(handles.location)),
                ("innerWidth".to_owned(), Value::Number(snapshot.inner_width)),
                (
                    "innerHeight".to_owned(),
                    Value::Number(snapshot.inner_height),
                ),
                (
                    "devicePixelRatio".to_owned(),
                    Value::Number(snapshot.device_pixel_ratio),
                ),
                (
                    "setTimeout".to_owned(),
                    Value::NativeFunction(handles.set_timeout),
                ),
                (
                    "clearTimeout".to_owned(),
                    Value::NativeFunction(handles.clear_timeout),
                ),
                (
                    "queueMicrotask".to_owned(),
                    Value::NativeFunction(handles.queue_microtask),
                ),
                ("Promise".to_owned(), Value::Object(handles.promise)),
                ("fetch".to_owned(), Value::NativeFunction(handles.fetch)),
                // PHANTOM_2M16_WEB_GLOBALS_CORE_BUILTINS_I
                ("console".to_owned(), Value::Object(handles.console)),
                ("Math".to_owned(), Value::Object(handles.math)),
                ("Array".to_owned(), Value::Object(handles.array)),
                ("Number".to_owned(), Value::Object(handles.number)),
                (
                    "parseInt".to_owned(),
                    Value::NativeFunction(handles.parse_int),
                ),
                (
                    "parseFloat".to_owned(),
                    Value::NativeFunction(handles.parse_float),
                ),
                ("isNaN".to_owned(), Value::NativeFunction(handles.is_nan)),
                ("NaN".to_owned(), Value::Number(f64::NAN)),
                ("Infinity".to_owned(), Value::Number(f64::INFINITY)),
                // PHANTOM_2M17_WEB_CORE_LIBRARY_II
                ("Object".to_owned(), Value::Object(handles.object)),
                ("JSON".to_owned(), Value::Object(handles.json)),
            ]),
        )?;

        self.set_global_host_binding("window", Value::Object(handles.window))?;
        self.set_global_host_binding("self", Value::Object(handles.window))?;
        self.set_global_host_binding("document", Value::Object(handles.document))?;
        self.set_global_host_binding("location", Value::Object(handles.location))?;
        self.set_global_host_binding("setTimeout", Value::NativeFunction(handles.set_timeout))?;
        self.set_global_host_binding("clearTimeout", Value::NativeFunction(handles.clear_timeout))?;
        self.set_global_host_binding(
            "queueMicrotask",
            Value::NativeFunction(handles.queue_microtask),
        )?;
        self.set_global_host_binding("Promise", Value::Object(handles.promise))?;
        self.set_global_host_binding("fetch", Value::NativeFunction(handles.fetch))?;
        // PHANTOM_2M16_WEB_GLOBALS_CORE_BUILTINS_I
        self.set_global_host_binding("console", Value::Object(handles.console))?;
        self.set_global_host_binding("Math", Value::Object(handles.math))?;
        self.set_global_host_binding("Array", Value::Object(handles.array))?;
        self.set_global_host_binding("Number", Value::Object(handles.number))?;
        self.set_global_host_binding("parseInt", Value::NativeFunction(handles.parse_int))?;
        self.set_global_host_binding("parseFloat", Value::NativeFunction(handles.parse_float))?;
        self.set_global_host_binding("isNaN", Value::NativeFunction(handles.is_nan))?;
        self.set_global_host_binding("NaN", Value::Number(f64::NAN))?;
        self.set_global_host_binding("Infinity", Value::Number(f64::INFINITY))?;
        // PHANTOM_2M17_WEB_CORE_LIBRARY_II
        self.set_global_host_binding("Object", Value::Object(handles.object))?;
        self.set_global_host_binding("JSON", Value::Object(handles.json))?;

        let materialized = self.host_element_objects.clone();
        for (node_id, object_id) in materialized {
            if let Some(element) = self.host_element(node_id).cloned() {
                self.refresh_host_element_object(object_id, &element)?;
            }
        }

        Ok(())
    }

    /// Drains DOM mutations emitted by the most recent execution.
    ///
    /// The queue is emptied atomically so the engine applies every command at
    /// most once.
    #[must_use]
    pub fn drain_dom_mutations(&mut self) -> Vec<BrowserDomMutation> {
        std::mem::take(&mut self.dom_mutations)
    }

    /// Rebinds temporary JavaScript DOM identifiers to real engine NodeIds.
    ///
    /// The engine calls this after applying a mutation batch containing
    /// `CreateElement`. Materialized JavaScript element identities remain
    /// stable while all host methods are redirected to the committed NodeId.
    pub fn remap_dom_node_ids(&mut self, mappings: &[(u64, u64)]) {
        for &(temporary, committed) in mappings {
            for element in &mut self.host_elements {
                if element.node_id == temporary {
                    element.node_id = committed;
                }
                if element.parent_node_id == Some(temporary) {
                    element.parent_node_id = Some(committed);
                }
            }

            if let Some(object_id) = self.host_element_objects.remove(&temporary) {
                self.host_element_objects.insert(committed, object_id);
            }

            for node_id in self.host_object_nodes.values_mut() {
                if *node_id == temporary {
                    *node_id = committed;
                }
            }
        }
    }

    fn execute_program(
        &mut self,
        source: &str,
        program: &Program,
    ) -> Result<Completion, RuntimeError> {
        self.execute_statements(source, program.statements())
    }

    fn execute_statements(
        &mut self,
        source: &str,
        statements: &[Statement],
    ) -> Result<Completion, RuntimeError> {
        let mut last = Value::Undefined;

        for statement in statements {
            match self.execute_statement(source, statement)? {
                Completion::Normal(value) => last = value,
                completion @ Completion::Return(_) => return Ok(completion),
            }
        }

        Ok(Completion::Normal(last))
    }

    fn execute_statement(
        &mut self,
        source: &str,
        statement: &Statement,
    ) -> Result<Completion, RuntimeError> {
        self.tick(statement.span().start())?;

        match statement.kind() {
            StatementKind::VariableDeclaration(declaration) => self
                .execute_variable_declaration(source, declaration)
                .map(Completion::Normal),
            StatementKind::Expression(expression) => self
                .evaluate_expression(source, expression)
                .map(Completion::Normal),
            StatementKind::Block(statements) => {
                self.push_block_scope(statement.span().start())?;
                let result = self.execute_statements(source, statements);
                self.pop_scope();
                result
            }
            StatementKind::If {
                test,
                consequent,
                alternate,
            } => {
                let condition = self.evaluate_expression(source, test)?;
                if is_truthy(&condition) {
                    self.execute_statement(source, consequent)
                } else if let Some(alternate) = alternate {
                    self.execute_statement(source, alternate)
                } else {
                    Ok(Completion::Normal(Value::Undefined))
                }
            }
            StatementKind::While { test, body } => {
                let mut last = Value::Undefined;
                loop {
                    let condition = self.evaluate_expression(source, test)?;
                    if !is_truthy(&condition) {
                        break;
                    }

                    match self.execute_statement(source, body)? {
                        Completion::Normal(value) => last = value,
                        completion @ Completion::Return(_) => return Ok(completion),
                    }
                }
                Ok(Completion::Normal(last))
            }
            StatementKind::For {
                initializer,
                test,
                update,
                body,
            } => self.execute_for(
                source,
                statement.span().start(),
                initializer.as_ref(),
                test.as_deref(),
                update.as_deref(),
                body,
            ),
            StatementKind::Return(expression) => {
                let value = match expression {
                    Some(expression) => self.evaluate_expression(source, expression)?,
                    None => Value::Undefined,
                };
                Ok(Completion::Return(value))
            }
        }
    }

    fn execute_for(
        &mut self,
        source: &str,
        offset: usize,
        initializer: Option<&ForInitializer>,
        test: Option<&Expression>,
        update: Option<&Expression>,
        body: &Statement,
    ) -> Result<Completion, RuntimeError> {
        self.push_block_scope(offset)?;

        let result = (|| {
            if let Some(initializer) = initializer {
                match initializer {
                    ForInitializer::VariableDeclaration(declaration) => {
                        self.execute_variable_declaration(source, declaration)?;
                    }
                    ForInitializer::Expression(expression) => {
                        self.evaluate_expression(source, expression)?;
                    }
                }
            }

            let mut last = Value::Undefined;

            loop {
                if let Some(test) = test {
                    let condition = self.evaluate_expression(source, test)?;
                    if !is_truthy(&condition) {
                        break;
                    }
                }

                match self.execute_statement(source, body)? {
                    Completion::Normal(value) => last = value,
                    completion @ Completion::Return(_) => return Ok(completion),
                }

                if let Some(update) = update {
                    self.evaluate_expression(source, update)?;
                }
            }

            Ok(Completion::Normal(last))
        })();

        self.pop_scope();
        result
    }

    fn execute_variable_declaration(
        &mut self,
        source: &str,
        declaration: &VariableDeclaration,
    ) -> Result<Value, RuntimeError> {
        let mut last = Value::Undefined;

        for declarator in declaration.declarators() {
            let name = self.span_text(source, declarator.name())?.to_owned();
            let value = match declarator.initializer() {
                Some(initializer) => self.evaluate_expression(source, initializer)?,
                None => Value::Undefined,
            };
            last = value.clone();

            match declaration.kind() {
                VariableKind::Let => {
                    self.declare_lexical(name, value, true, declarator.name().start())?;
                }
                VariableKind::Const => {
                    self.declare_lexical(name, value, false, declarator.name().start())?;
                }
                VariableKind::Var => {
                    self.declare_var(
                        name,
                        value,
                        declarator.initializer().is_some(),
                        declarator.name().start(),
                    )?;
                }
            }
        }

        Ok(last)
    }

    fn evaluate_expression(
        &mut self,
        source: &str,
        expression: &Expression,
    ) -> Result<Value, RuntimeError> {
        self.tick(expression.span().start())?;

        match expression.kind() {
            ExpressionKind::Identifier => {
                let name = self.span_text(source, expression.span())?;
                if name == "undefined" {
                    Ok(Value::Undefined)
                } else {
                    self.lookup(name, expression.span().start())
                }
            }
            ExpressionKind::NumberLiteral => {
                let text = self.span_text(source, expression.span())?;
                text.parse::<f64>().map(Value::Number).map_err(|_| {
                    RuntimeError::new(
                        RuntimeErrorKind::InvalidNumberLiteral,
                        expression.span().start(),
                    )
                })
            }
            ExpressionKind::StringLiteral => {
                let text = self.span_text(source, expression.span())?;
                decode_string_literal(text, expression.span().start())
            }
            ExpressionKind::Boolean(value) => Ok(Value::Boolean(*value)),
            ExpressionKind::Null => Ok(Value::Null),
            ExpressionKind::This => Ok(self
                .scopes
                .iter()
                .rev()
                .find_map(|scope| scope.bindings.get("this"))
                .map(|binding| binding.value.clone())
                .or_else(|| {
                    self.browser_host
                        .map(|handles| Value::Object(handles.window))
                })
                .unwrap_or(Value::Undefined)),
            ExpressionKind::Grouped(expression) => self.evaluate_expression(source, expression),
            ExpressionKind::Unary { operator, argument } => {
                let value = self.evaluate_expression(source, argument)?;
                Ok(evaluate_unary(*operator, value))
            }
            ExpressionKind::Binary {
                left,
                operator,
                right,
            } => self.evaluate_binary(source, left, *operator, right),
            ExpressionKind::ArrayLiteral(elements) => {
                if elements.len() > MAX_ARRAY_LENGTH {
                    return Err(RuntimeError::new(
                        RuntimeErrorKind::ArrayLimitExceeded,
                        expression.span().start(),
                    ));
                }

                let mut values = Vec::with_capacity(elements.len());
                for element in elements {
                    values.push(self.evaluate_expression(source, element)?);
                }
                self.allocate_array(values, expression.span().start())
            }
            ExpressionKind::ObjectLiteral(properties) => {
                let mut object = BTreeMap::new();

                for property in properties {
                    let key = self.object_literal_key(source, property.key())?;
                    let value = self.evaluate_expression(source, property.value())?;
                    if !object.contains_key(&key) && object.len() >= MAX_OBJECT_PROPERTIES {
                        return Err(RuntimeError::new(
                            RuntimeErrorKind::PropertyLimitExceeded,
                            property.span().start(),
                        ));
                    }
                    object.insert(key, value);
                }

                self.allocate_object(object, expression.span().start())
            }
            ExpressionKind::Member { object, property } => {
                let target = self.evaluate_expression(source, object)?;
                let key = self.member_key(source, property)?;
                self.get_member(&target, &key, expression.span().start())
            }
            ExpressionKind::Call { callee, arguments } => {
                if arguments.len() > MAX_FUNCTION_ARGUMENTS {
                    return Err(RuntimeError::new(
                        RuntimeErrorKind::ArgumentLimitExceeded,
                        expression.span().start(),
                    ));
                }

                // PHANTOM_2M7_R1_FUNCTION_CONTEXT_RUNTIME_I
                // Member calls carry their receiver into the function frame so
                // `obj.method()` can resolve `this` to `obj`. Plain calls keep
                // the bounded foundation behavior of an undefined receiver.
                let (callable, this_value) =
                    if let ExpressionKind::Member { object, property } = callee.kind() {
                        let target = self.evaluate_expression(source, object)?;
                        let key = self.member_key(source, property)?;
                        let callable = self.get_member(&target, &key, callee.span().start())?;
                        (callable, target)
                    } else {
                        (self.evaluate_expression(source, callee)?, Value::Undefined)
                    };
                let mut values = Vec::with_capacity(arguments.len());
                for argument in arguments {
                    values.push(self.evaluate_expression(source, argument)?);
                }
                self.call_value_with_this(callable, values, this_value, expression.span().start())
            }
            ExpressionKind::Function(function) => {
                let name = function
                    .name()
                    .map(|span| self.span_text(source, span).map(str::to_owned))
                    .transpose()?;

                let parameters = function
                    .parameters()
                    .iter()
                    .map(|span| self.span_text(source, *span).map(str::to_owned))
                    .collect::<Result<Vec<_>, _>>()?;

                let captured_scopes = self.capture_function_scopes(expression.span().start())?;
                self.allocate_function(
                    FunctionRecord {
                        name,
                        parameters,
                        body: function.body().to_vec(),
                        source: source.to_owned(),
                        captured_scopes,
                    },
                    expression.span().start(),
                )
            }
            ExpressionKind::Conditional {
                test,
                consequent,
                alternate,
            } => {
                let condition = self.evaluate_expression(source, test)?;
                if is_truthy(&condition) {
                    self.evaluate_expression(source, consequent)
                } else {
                    self.evaluate_expression(source, alternate)
                }
            }
            ExpressionKind::Assignment {
                left,
                operator: AssignmentOperator::Assign,
                right,
            } => match left.kind() {
                ExpressionKind::Identifier => {
                    let value = self.evaluate_expression(source, right)?;
                    let name = self.span_text(source, left.span())?.to_owned();
                    self.assign(&name, value.clone(), left.span().start())?;
                    Ok(value)
                }
                ExpressionKind::Member { object, property } => {
                    let target = self.evaluate_expression(source, object)?;
                    let key = self.member_key(source, property)?;
                    let value = self.evaluate_expression(source, right)?;
                    self.set_member(&target, &key, value.clone(), left.span().start())?;
                    Ok(value)
                }
                _ => Err(RuntimeError::new(
                    RuntimeErrorKind::UnsupportedAssignmentTarget,
                    left.span().start(),
                )),
            },
        }
    }

    fn evaluate_binary(
        &mut self,
        source: &str,
        left: &Expression,
        operator: BinaryOperator,
        right: &Expression,
    ) -> Result<Value, RuntimeError> {
        let left_value = self.evaluate_expression(source, left)?;

        match operator {
            BinaryOperator::LogicalAnd => {
                if !is_truthy(&left_value) {
                    return Ok(left_value);
                }
                return self.evaluate_expression(source, right);
            }
            BinaryOperator::LogicalOr => {
                if is_truthy(&left_value) {
                    return Ok(left_value);
                }
                return self.evaluate_expression(source, right);
            }
            _ => {}
        }

        let right_value = self.evaluate_expression(source, right)?;

        match operator {
            BinaryOperator::Multiply => Ok(Value::Number(
                to_number(&left_value) * to_number(&right_value),
            )),
            BinaryOperator::Divide => Ok(Value::Number(
                to_number(&left_value) / to_number(&right_value),
            )),
            BinaryOperator::Remainder => Ok(Value::Number(
                to_number(&left_value) % to_number(&right_value),
            )),
            BinaryOperator::Add => self.add_values(left_value, right_value),
            BinaryOperator::Subtract => Ok(Value::Number(
                to_number(&left_value) - to_number(&right_value),
            )),
            BinaryOperator::Less => Ok(Value::Boolean(relational_compare(
                &left_value,
                &right_value,
                |left, right| left < right,
            ))),
            BinaryOperator::LessEqual => Ok(Value::Boolean(relational_compare(
                &left_value,
                &right_value,
                |left, right| left <= right,
            ))),
            BinaryOperator::Greater => Ok(Value::Boolean(relational_compare(
                &left_value,
                &right_value,
                |left, right| left > right,
            ))),
            BinaryOperator::GreaterEqual => Ok(Value::Boolean(relational_compare(
                &left_value,
                &right_value,
                |left, right| left >= right,
            ))),
            BinaryOperator::Equal => Ok(Value::Boolean(abstract_equal(&left_value, &right_value))),
            BinaryOperator::NotEqual => {
                Ok(Value::Boolean(!abstract_equal(&left_value, &right_value)))
            }
            BinaryOperator::StrictEqual => {
                Ok(Value::Boolean(strict_equal(&left_value, &right_value)))
            }
            BinaryOperator::StrictNotEqual => {
                Ok(Value::Boolean(!strict_equal(&left_value, &right_value)))
            }
            BinaryOperator::LogicalAnd | BinaryOperator::LogicalOr => Ok(Value::Undefined),
        }
    }

    fn add_values(&self, left: Value, right: Value) -> Result<Value, RuntimeError> {
        if matches!(left, Value::String(_)) || matches!(right, Value::String(_)) {
            let value = format!("{}{}", runtime_string(&left), runtime_string(&right));
            if value.len() > MAX_RUNTIME_STRING_BYTES {
                return Err(RuntimeError::new(RuntimeErrorKind::InvalidStringLiteral, 0));
            }
            Ok(Value::String(value))
        } else {
            Ok(Value::Number(to_number(&left) + to_number(&right)))
        }
    }

    fn object_literal_key(&self, source: &str, span: Span) -> Result<String, RuntimeError> {
        let raw = self.span_text(source, span)?;
        if raw.starts_with('"') || raw.starts_with('\'') {
            let value = decode_string_literal(raw, span.start())?;
            let Value::String(value) = value else {
                return Err(RuntimeError::new(
                    RuntimeErrorKind::InvalidPropertyKey,
                    span.start(),
                ));
            };
            Ok(value)
        } else {
            Ok(raw.to_owned())
        }
    }

    fn member_key(
        &mut self,
        source: &str,
        property: &MemberProperty,
    ) -> Result<String, RuntimeError> {
        match property {
            MemberProperty::Identifier(span) => Ok(self.span_text(source, *span)?.to_owned()),
            MemberProperty::Computed(expression) => {
                let value = self.evaluate_expression(source, expression)?;
                property_key(&value, expression.span().start())
            }
        }
    }

    fn allocate_array(&mut self, values: Vec<Value>, offset: usize) -> Result<Value, RuntimeError> {
        if self.arrays.len() >= MAX_ARRAYS || values.len() > MAX_ARRAY_LENGTH {
            return Err(RuntimeError::new(
                RuntimeErrorKind::ArrayLimitExceeded,
                offset,
            ));
        }

        let id = self.arrays.len();
        self.arrays.push(values);
        Ok(Value::Array(id))
    }

    fn allocate_object(
        &mut self,
        values: BTreeMap<String, Value>,
        offset: usize,
    ) -> Result<Value, RuntimeError> {
        if self.objects.len() >= MAX_OBJECTS {
            return Err(RuntimeError::new(
                RuntimeErrorKind::ObjectLimitExceeded,
                offset,
            ));
        }
        if values.len() > MAX_OBJECT_PROPERTIES {
            return Err(RuntimeError::new(
                RuntimeErrorKind::PropertyLimitExceeded,
                offset,
            ));
        }

        let id = self.objects.len();
        self.objects.push(values);
        Ok(Value::Object(id))
    }

    fn capture_function_scopes(&self, offset: usize) -> Result<Vec<Scope>, RuntimeError> {
        let captured_scopes = self.scopes.iter().skip(1).cloned().collect::<Vec<_>>();
        let captured_bindings = captured_scopes.iter().fold(0usize, |total, scope| {
            total.saturating_add(scope.bindings.len())
        });
        if captured_bindings > MAX_CAPTURED_BINDINGS_PER_FUNCTION {
            return Err(RuntimeError::new(
                RuntimeErrorKind::BindingLimitExceeded,
                offset,
            ));
        }
        Ok(captured_scopes)
    }

    fn allocate_function(
        &mut self,
        function: FunctionRecord,
        offset: usize,
    ) -> Result<Value, RuntimeError> {
        if self.functions.len() >= MAX_FUNCTIONS {
            return Err(RuntimeError::new(
                RuntimeErrorKind::FunctionLimitExceeded,
                offset,
            ));
        }

        let retained = self
            .retained_function_source_bytes
            .saturating_add(function.source.len());
        if retained > MAX_RETAINED_FUNCTION_SOURCE_BYTES {
            return Err(RuntimeError::new(
                RuntimeErrorKind::RetainedSourceLimitExceeded,
                offset,
            ));
        }

        let id = self.functions.len();
        self.functions.push(function);
        self.retained_function_source_bytes = retained;
        Ok(Value::Function(id))
    }

    fn get_member(
        &mut self,
        target: &Value,
        key: &str,
        offset: usize,
    ) -> Result<Value, RuntimeError> {
        match target {
            Value::Array(id) => {
                let Some(array) = self.arrays.get(*id) else {
                    return Err(RuntimeError::new(
                        RuntimeErrorKind::InvalidMemberTarget,
                        offset,
                    ));
                };

                if key == "length" {
                    return Ok(Value::Number(array.len() as f64));
                }
                // PHANTOM_2M17_WEB_CORE_LIBRARY_II
                if let Some(handles) = self.browser_host {
                    let method = match key {
                        "push" => Some(handles.array_push),
                        "pop" => Some(handles.array_pop),
                        "join" => Some(handles.array_join),
                        "indexOf" => Some(handles.array_index_of),
                        "includes" => Some(handles.array_includes),
                        "slice" => Some(handles.array_slice),
                        // PHANTOM_2M18_WEB_CORE_LIBRARY_III
                        "forEach" => Some(handles.array_for_each),
                        "map" => Some(handles.array_map),
                        "filter" => Some(handles.array_filter),
                        "some" => Some(handles.array_some),
                        "every" => Some(handles.array_every),
                        "find" => Some(handles.array_find),
                        "findIndex" => Some(handles.array_find_index),
                        "reduce" => Some(handles.array_reduce),
                        // PHANTOM_2M19_WEB_CORE_LIBRARY_FINAL_GATE
                        "at" => Some(handles.array_at),
                        "shift" => Some(handles.array_shift),
                        "unshift" => Some(handles.array_unshift),
                        "concat" => Some(handles.array_concat),
                        "reverse" => Some(handles.array_reverse),
                        _ => None,
                    };
                    if let Some(method) = method {
                        return Ok(Value::NativeFunction(method));
                    }
                }

                let Some(index) = canonical_array_index(key) else {
                    return Ok(Value::Undefined);
                };
                Ok(array.get(index).cloned().unwrap_or(Value::Undefined))
            }
            Value::Object(id) => {
                // PHANTOM_2M20_DOM_TREE_NAVIGATION_MUTATION_COHERENCE_I
                // Element tree-navigation properties are resolved lazily from the
                // live bounded host snapshot so reparent/remove operations inside
                // the same JavaScript execution are immediately observable.
                if self.host_object_nodes.contains_key(id)
                    && matches!(
                        key,
                        "parentElement"
                            | "children"
                            | "childElementCount"
                            | "firstElementChild"
                            | "lastElementChild"
                            | "previousElementSibling"
                            | "nextElementSibling"
                            | "isConnected"
                    )
                {
                    return self.host_tree_navigation_member(*id, key, offset);
                }

                let Some(object) = self.objects.get(*id) else {
                    return Err(RuntimeError::new(
                        RuntimeErrorKind::InvalidMemberTarget,
                        offset,
                    ));
                };

                if let Some(value) = object.get(key).cloned() {
                    return Ok(value);
                }

                // PHANTOM_2M13_CSS_STYLE_DECLARATION_I
                // CSSStyleDeclaration named properties are live views over the
                // element's current `style` attribute instead of stale object slots.
                if let Some(element_object_id) = self.host_style_object_elements.get(id).copied() {
                    let Some(node_id) = self.host_object_nodes.get(&element_object_id).copied()
                    else {
                        return Err(RuntimeError::new(
                            RuntimeErrorKind::InvalidMemberTarget,
                            offset,
                        ));
                    };
                    if key == "cssText" {
                        return self.host_style_css_text(node_id, offset).map(Value::String);
                    }
                    let Ok(property_name) = style_property_name_from_member(key, offset) else {
                        return Ok(Value::Undefined);
                    };
                    return self
                        .host_style_property_value(node_id, &property_name, offset)
                        .map(Value::String);
                }

                // PHANTOM_2M14_DOM_DATASET_ATTRIBUTE_REFLECTION_I
                if let Some(element_object_id) = self.host_dataset_object_elements.get(id).copied()
                {
                    let Some(node_id) = self.host_object_nodes.get(&element_object_id).copied()
                    else {
                        return Err(RuntimeError::new(
                            RuntimeErrorKind::InvalidMemberTarget,
                            offset,
                        ));
                    };
                    let Some(attribute_name) = dataset_attribute_name_from_member(key) else {
                        return Ok(Value::Undefined);
                    };
                    return Ok(self
                        .host_element(node_id)
                        .and_then(|element| element.attributes.get(&attribute_name))
                        .cloned()
                        .map(Value::String)
                        .unwrap_or(Value::Undefined));
                }

                Ok(Value::Undefined)
            }
            // PHANTOM_2M17_WEB_CORE_LIBRARY_II
            Value::String(value) => {
                if key == "length" {
                    return Ok(Value::Number(value.chars().count() as f64));
                }
                if let Some(index) = canonical_array_index(key) {
                    return Ok(value
                        .chars()
                        .nth(index)
                        .map(|character| Value::String(character.to_string()))
                        .unwrap_or(Value::Undefined));
                }
                if let Some(handles) = self.browser_host {
                    let method = match key {
                        "trim" => Some(handles.string_trim),
                        "toLowerCase" => Some(handles.string_to_lower_case),
                        "toUpperCase" => Some(handles.string_to_upper_case),
                        "includes" => Some(handles.string_includes),
                        "startsWith" => Some(handles.string_starts_with),
                        "endsWith" => Some(handles.string_ends_with),
                        "indexOf" => Some(handles.string_index_of),
                        "slice" => Some(handles.string_slice),
                        // PHANTOM_2M18_WEB_CORE_LIBRARY_III
                        "split" => Some(handles.string_split),
                        "replace" => Some(handles.string_replace),
                        // PHANTOM_2M19_WEB_CORE_LIBRARY_FINAL_GATE
                        "charAt" => Some(handles.string_char_at),
                        "substring" => Some(handles.string_substring),
                        "repeat" => Some(handles.string_repeat),
                        "padStart" => Some(handles.string_pad_start),
                        "padEnd" => Some(handles.string_pad_end),
                        "replaceAll" => Some(handles.string_replace_all),
                        _ => None,
                    };
                    if let Some(method) = method {
                        return Ok(Value::NativeFunction(method));
                    }
                }
                Ok(Value::Undefined)
            }
            _ => Err(RuntimeError::new(
                RuntimeErrorKind::InvalidMemberTarget,
                offset,
            )),
        }
    }

    fn set_member(
        &mut self,
        target: &Value,
        key: &str,
        value: Value,
        offset: usize,
    ) -> Result<(), RuntimeError> {
        match target {
            Value::Array(id) => {
                if key == "length" {
                    return Err(RuntimeError::new(
                        RuntimeErrorKind::InvalidPropertyKey,
                        offset,
                    ));
                }

                let Some(index) = canonical_array_index(key) else {
                    return Err(RuntimeError::new(
                        RuntimeErrorKind::InvalidPropertyKey,
                        offset,
                    ));
                };
                if index >= MAX_ARRAY_LENGTH {
                    return Err(RuntimeError::new(
                        RuntimeErrorKind::ArrayLimitExceeded,
                        offset,
                    ));
                }

                let Some(array) = self.arrays.get_mut(*id) else {
                    return Err(RuntimeError::new(
                        RuntimeErrorKind::InvalidMemberTarget,
                        offset,
                    ));
                };

                if index >= array.len() {
                    array.resize(index.saturating_add(1), Value::Undefined);
                }
                array[index] = value;
                Ok(())
            }
            Value::Object(id) => {
                // PHANTOM_2M13_CSS_STYLE_DECLARATION_I
                if let Some(element_object_id) = self.host_style_object_elements.get(id).copied() {
                    let Some(node_id) = self.host_object_nodes.get(&element_object_id).copied()
                    else {
                        return Err(RuntimeError::new(
                            RuntimeErrorKind::InvalidMemberTarget,
                            offset,
                        ));
                    };
                    if key == "cssText" {
                        return self.set_host_style_css_text(
                            node_id,
                            runtime_string(&value),
                            offset,
                        );
                    }
                    if matches!(key, "setProperty" | "getPropertyValue" | "removeProperty") {
                        return Err(RuntimeError::new(
                            RuntimeErrorKind::InvalidPropertyKey,
                            offset,
                        ));
                    }
                    let property_name = style_property_name_from_member(key, offset)?;
                    return self.set_host_style_property(
                        node_id,
                        &property_name,
                        runtime_string(&value),
                        offset,
                    );
                }

                // PHANTOM_2M14_DOM_DATASET_ATTRIBUTE_REFLECTION_I
                if let Some(element_object_id) = self.host_dataset_object_elements.get(id).copied()
                {
                    let Some(node_id) = self.host_object_nodes.get(&element_object_id).copied()
                    else {
                        return Err(RuntimeError::new(
                            RuntimeErrorKind::InvalidMemberTarget,
                            offset,
                        ));
                    };
                    let Some(attribute_name) = dataset_attribute_name_from_member(key) else {
                        return Err(RuntimeError::new(
                            RuntimeErrorKind::InvalidPropertyKey,
                            offset,
                        ));
                    };
                    let attribute_value = runtime_string(&value);
                    self.update_host_attribute(
                        node_id,
                        &attribute_name,
                        Some(attribute_value.clone()),
                        offset,
                    )?;
                    self.queue_dom_mutation(
                        BrowserDomMutation::SetAttribute {
                            node_id,
                            name: attribute_name,
                            value: attribute_value,
                        },
                        offset,
                    )?;
                    return Ok(());
                }

                if let Some(node_id) = self.host_object_nodes.get(id).copied() {
                    if key == "textContent" {
                        let text = runtime_string(&value);
                        self.update_host_text_content(node_id, text.clone(), offset)?;
                        self.queue_dom_mutation(
                            BrowserDomMutation::SetTextContent {
                                node_id,
                                value: text,
                            },
                            offset,
                        )?;
                        return Ok(());
                    }

                    if key == "id" {
                        let id_value = runtime_string(&value);
                        self.update_host_attribute(node_id, "id", Some(id_value.clone()), offset)?;
                        self.queue_dom_mutation(
                            BrowserDomMutation::SetAttribute {
                                node_id,
                                name: "id".to_owned(),
                                value: id_value,
                            },
                            offset,
                        )?;
                        return Ok(());
                    }

                    if key == "className" {
                        let class_value = runtime_string(&value);
                        self.update_host_attribute(
                            node_id,
                            "class",
                            Some(class_value.clone()),
                            offset,
                        )?;
                        self.queue_dom_mutation(
                            BrowserDomMutation::SetAttribute {
                                node_id,
                                name: "class".to_owned(),
                                value: class_value,
                            },
                            offset,
                        )?;
                        return Ok(());
                    }

                    // PHANTOM_2M15_HTML_ATTRIBUTE_PROPERTY_REFLECTION_II
                    if matches!(key, "title" | "lang" | "dir") {
                        let attribute_value = runtime_string(&value);
                        self.update_host_attribute(
                            node_id,
                            key,
                            Some(attribute_value.clone()),
                            offset,
                        )?;
                        self.queue_dom_mutation(
                            BrowserDomMutation::SetAttribute {
                                node_id,
                                name: key.to_owned(),
                                value: attribute_value,
                            },
                            offset,
                        )?;
                        return Ok(());
                    }

                    if key == "hidden" {
                        let hidden = is_truthy(&value);
                        let present = self
                            .host_element(node_id)
                            .is_some_and(|element| element.attributes.contains_key("hidden"));
                        if hidden != present {
                            if hidden {
                                self.update_host_attribute(
                                    node_id,
                                    "hidden",
                                    Some(String::new()),
                                    offset,
                                )?;
                                self.queue_dom_mutation(
                                    BrowserDomMutation::SetAttribute {
                                        node_id,
                                        name: "hidden".to_owned(),
                                        value: String::new(),
                                    },
                                    offset,
                                )?;
                            } else {
                                self.update_host_attribute(node_id, "hidden", None, offset)?;
                                self.queue_dom_mutation(
                                    BrowserDomMutation::RemoveAttribute {
                                        node_id,
                                        name: "hidden".to_owned(),
                                    },
                                    offset,
                                )?;
                            }
                        }
                        return Ok(());
                    }

                    // PHANTOM_2M23_HTML_ELEMENT_INTERACTION_I
                    if let Some(attribute_name) = html_string_reflected_attribute(key) {
                        let attribute_value = runtime_string(&value);
                        self.update_host_attribute(
                            node_id,
                            attribute_name,
                            Some(attribute_value.clone()),
                            offset,
                        )?;
                        self.queue_dom_mutation(
                            BrowserDomMutation::SetAttribute {
                                node_id,
                                name: attribute_name.to_owned(),
                                value: attribute_value,
                            },
                            offset,
                        )?;
                        return Ok(());
                    }

                    if let Some(attribute_name) = html_boolean_reflected_attribute(key) {
                        let enabled = is_truthy(&value);
                        let present = self
                            .host_element(node_id)
                            .is_some_and(|element| element.attributes.contains_key(attribute_name));
                        if enabled != present {
                            if enabled {
                                self.update_host_attribute(
                                    node_id,
                                    attribute_name,
                                    Some(String::new()),
                                    offset,
                                )?;
                                self.queue_dom_mutation(
                                    BrowserDomMutation::SetAttribute {
                                        node_id,
                                        name: attribute_name.to_owned(),
                                        value: String::new(),
                                    },
                                    offset,
                                )?;
                            } else {
                                self.update_host_attribute(node_id, attribute_name, None, offset)?;
                                self.queue_dom_mutation(
                                    BrowserDomMutation::RemoveAttribute {
                                        node_id,
                                        name: attribute_name.to_owned(),
                                    },
                                    offset,
                                )?;
                            }
                        }
                        return Ok(());
                    }
                }

                let Some(object) = self.objects.get_mut(*id) else {
                    return Err(RuntimeError::new(
                        RuntimeErrorKind::InvalidMemberTarget,
                        offset,
                    ));
                };

                if !object.contains_key(key) && object.len() >= MAX_OBJECT_PROPERTIES {
                    return Err(RuntimeError::new(
                        RuntimeErrorKind::PropertyLimitExceeded,
                        offset,
                    ));
                }

                object.insert(key.to_owned(), value);
                Ok(())
            }
            _ => Err(RuntimeError::new(
                RuntimeErrorKind::InvalidMemberTarget,
                offset,
            )),
        }
    }

    fn call_value(
        &mut self,
        callable: Value,
        arguments: Vec<Value>,
        offset: usize,
    ) -> Result<Value, RuntimeError> {
        self.call_value_with_this(callable, arguments, Value::Undefined, offset)
    }

    fn call_value_with_this(
        &mut self,
        callable: Value,
        arguments: Vec<Value>,
        this_value: Value,
        offset: usize,
    ) -> Result<Value, RuntimeError> {
        match callable {
            Value::Function(function_id) => {
                self.call_function(function_id, arguments, this_value, offset)
            }
            Value::NativeFunction(function_id) => {
                self.call_native_function(function_id, arguments, this_value, offset)
            }
            Value::HostFunction(handle) => self.call_host_function(handle, arguments, offset),
            _ => Err(RuntimeError::new(RuntimeErrorKind::CallNonFunction, offset)),
        }
    }

    fn allocate_native_function(
        &mut self,
        function: NativeFunctionKind,
        offset: usize,
    ) -> Result<usize, RuntimeError> {
        if self.native_functions.len() >= MAX_NATIVE_FUNCTIONS {
            return Err(RuntimeError::new(
                RuntimeErrorKind::NativeFunctionLimitExceeded,
                offset,
            ));
        }

        let id = self.native_functions.len();
        self.native_functions.push(function);
        Ok(id)
    }

    fn call_native_function(
        &mut self,
        function_id: usize,
        arguments: Vec<Value>,
        this_value: Value,
        offset: usize,
    ) -> Result<Value, RuntimeError> {
        let Some(function) = self.native_functions.get(function_id).cloned() else {
            return Err(RuntimeError::new(RuntimeErrorKind::CallNonFunction, offset));
        };

        match function {
            NativeFunctionKind::GetElementById => {
                let key = arguments.first().map(runtime_string).unwrap_or_default();
                let node_id = self.host_elements.iter().find_map(|element| {
                    (element.connected
                        && element.attributes.get("id").map(String::as_str) == Some(key.as_str()))
                    .then_some(element.node_id)
                });

                match node_id {
                    Some(node_id) => self.materialize_host_element(node_id, offset),
                    None => Ok(Value::Null),
                }
            }
            NativeFunctionKind::QuerySelector => {
                let selector = arguments.first().map(runtime_string).unwrap_or_default();
                let node_id = self.query_host_selector(&selector, offset)?;
                match node_id {
                    Some(node_id) => self.materialize_host_element(node_id, offset),
                    None => Ok(Value::Null),
                }
            }
            NativeFunctionKind::QuerySelectorAll => {
                let selector = arguments.first().map(runtime_string).unwrap_or_default();
                let node_ids = self.query_host_selector_all(&selector, None, true, offset)?;
                self.materialize_host_collection(&node_ids, offset)
            }
            NativeFunctionKind::GetElementsByClassName => {
                let classes = arguments.first().map(runtime_string).unwrap_or_default();
                let class_names = parse_host_class_names(&classes, offset)?;
                let node_ids = self
                    .host_elements
                    .iter()
                    .filter_map(|element| {
                        (element.connected && host_has_all_classes(element, &class_names))
                            .then_some(element.node_id)
                    })
                    .collect::<Vec<_>>();
                self.materialize_host_collection(&node_ids, offset)
            }
            NativeFunctionKind::GetElementsByTagName => {
                let tag_name = arguments
                    .first()
                    .map(runtime_string)
                    .unwrap_or_default()
                    .trim()
                    .to_ascii_lowercase();
                if tag_name != "*" && !valid_host_tag_name(&tag_name) {
                    return Err(RuntimeError::new(
                        RuntimeErrorKind::InvalidHostTagName,
                        offset,
                    ));
                }
                let node_ids = self
                    .host_elements
                    .iter()
                    .filter_map(|element| {
                        (element.connected
                            && (tag_name == "*"
                                || element.tag_name.eq_ignore_ascii_case(&tag_name)))
                        .then_some(element.node_id)
                    })
                    .collect::<Vec<_>>();
                self.materialize_host_collection(&node_ids, offset)
            }
            NativeFunctionKind::CreateElement => {
                let tag_name = arguments
                    .first()
                    .map(runtime_string)
                    .unwrap_or_default()
                    .trim()
                    .to_ascii_lowercase();

                if !valid_host_tag_name(&tag_name) {
                    return Err(RuntimeError::new(
                        RuntimeErrorKind::InvalidHostTagName,
                        offset,
                    ));
                }

                if self.host_elements.len() >= MAX_HOST_ELEMENTS {
                    return Err(RuntimeError::new(
                        RuntimeErrorKind::HostSnapshotLimitExceeded,
                        offset,
                    ));
                }

                let node_id = self.allocate_virtual_node_id(offset)?;
                self.host_elements.push(BrowserDomElementSnapshot {
                    node_id,
                    tag_name: tag_name.clone(),
                    attributes: BTreeMap::new(),
                    text_content: String::new(),
                    parent_node_id: None,
                    connected: false,
                });
                self.queue_dom_mutation(
                    BrowserDomMutation::CreateElement { node_id, tag_name },
                    offset,
                )?;
                self.materialize_host_element(node_id, offset)
            }
            NativeFunctionKind::SetTimeout => {
                if arguments.len() > 2 {
                    return Err(RuntimeError::new(
                        RuntimeErrorKind::UnsupportedTimerArguments,
                        offset,
                    ));
                }

                let Some(Value::Function(callback)) = arguments.first() else {
                    return Err(RuntimeError::new(
                        RuntimeErrorKind::InvalidTimerCallback,
                        offset,
                    ));
                };

                let delay_ms = timer_delay_ms(arguments.get(1));
                self.schedule_timeout(*callback, delay_ms, offset)
                    .map(|timer_id| Value::Number(f64::from(timer_id)))
            }
            NativeFunctionKind::ClearTimeout => {
                if let Some(timer_id) = arguments.first().and_then(timer_id_from_value) {
                    self.timers.retain(|timer| timer.id != timer_id);
                }
                Ok(Value::Undefined)
            }
            NativeFunctionKind::QueueMicrotask => {
                let Some(callback) = arguments
                    .first()
                    .filter(|value| is_callable(value))
                    .cloned()
                else {
                    return Err(RuntimeError::new(
                        RuntimeErrorKind::InvalidMicrotaskCallback,
                        offset,
                    ));
                };

                self.enqueue_microtask(
                    Microtask::Call {
                        callback,
                        arguments: Vec::new(),
                    },
                    offset,
                )?;
                Ok(Value::Undefined)
            }
            NativeFunctionKind::PromiseResolve => {
                let value = arguments.first().cloned().unwrap_or(Value::Undefined);
                if let Value::Object(object_id) = value
                    && self.promises.contains_key(&object_id)
                {
                    return Ok(Value::Object(object_id));
                }

                let promise = self.allocate_promise(PromiseState::Pending, offset)?;
                self.settle_promise(promise, PromiseState::Fulfilled(value), offset)?;
                Ok(Value::Object(promise))
            }
            NativeFunctionKind::PromiseReject => {
                let reason = arguments.first().cloned().unwrap_or(Value::Undefined);
                self.allocate_promise(PromiseState::Rejected(reason), offset)
                    .map(Value::Object)
            }
            NativeFunctionKind::PromiseThen(promise_id) => {
                let on_fulfilled = arguments
                    .first()
                    .filter(|value| is_callable(value))
                    .cloned();
                let on_rejected = arguments.get(1).filter(|value| is_callable(value)).cloned();
                self.register_promise_reaction(promise_id, on_fulfilled, on_rejected, offset)
                    .map(Value::Object)
            }
            NativeFunctionKind::PromiseCatch(promise_id) => {
                let on_rejected = arguments
                    .first()
                    .filter(|value| is_callable(value))
                    .cloned();
                self.register_promise_reaction(promise_id, None, on_rejected, offset)
                    .map(Value::Object)
            }
            NativeFunctionKind::ThenableResolve(promise_id, thenable_id) => {
                let value = arguments.first().cloned().unwrap_or(Value::Undefined);
                let state = if matches!(&value, Value::Object(object_id) if *object_id == thenable_id)
                {
                    PromiseState::Rejected(Value::String(
                        "thenable cannot resolve itself".to_owned(),
                    ))
                } else {
                    PromiseState::Fulfilled(value)
                };
                self.settle_promise(promise_id, state, offset)?;
                Ok(Value::Undefined)
            }
            NativeFunctionKind::ThenableReject(promise_id) => {
                let reason = arguments.first().cloned().unwrap_or(Value::Undefined);
                self.settle_promise(promise_id, PromiseState::Rejected(reason), offset)?;
                Ok(Value::Undefined)
            }
            // PHANTOM_2M10_DOM_EVENT_CONTROL_III
            NativeFunctionKind::EventPreventDefault(event_id) => {
                // PHANTOM_2M11_DOM_EVENT_LISTENER_SEMANTICS_IV
                // DOM passive listeners are observational: preventDefault() is ignored
                // while a passive listener is the active callback.
                let can_prevent = self
                    .event_dispatch_controls
                    .iter()
                    .rev()
                    .find(|control| control.event_object == event_id)
                    .is_some_and(|control| control.cancelable && !control.in_passive_listener);
                if can_prevent {
                    if let Some(control) = self
                        .event_dispatch_controls
                        .iter_mut()
                        .rev()
                        .find(|control| control.event_object == event_id)
                    {
                        control.default_prevented = true;
                    }
                    if let Some(event) = self.objects.get_mut(event_id) {
                        event.insert("defaultPrevented".to_owned(), Value::Boolean(true));
                    }
                }
                Ok(Value::Undefined)
            }
            NativeFunctionKind::EventStopPropagation(event_id) => {
                if let Some(control) = self
                    .event_dispatch_controls
                    .iter_mut()
                    .rev()
                    .find(|control| control.event_object == event_id)
                {
                    control.propagation_stopped = true;
                }
                Ok(Value::Undefined)
            }
            NativeFunctionKind::EventStopImmediatePropagation(event_id) => {
                if let Some(control) = self
                    .event_dispatch_controls
                    .iter_mut()
                    .rev()
                    .find(|control| control.event_object == event_id)
                {
                    control.propagation_stopped = true;
                    control.immediate_stopped = true;
                }
                Ok(Value::Undefined)
            }
            NativeFunctionKind::Fetch => {
                if arguments.len() > 2 {
                    return Err(RuntimeError::new(
                        RuntimeErrorKind::UnsupportedFetchOptions,
                        offset,
                    ));
                }

                let Some(Value::String(url)) = arguments.first() else {
                    return Err(RuntimeError::new(RuntimeErrorKind::InvalidFetchUrl, offset));
                };

                let init = self.parse_fetch_request_init(arguments.get(1), offset)?;
                self.issue_fetch(url.clone(), init, offset)
                    .map(Value::Object)
            }
            NativeFunctionKind::ResponseText(response_id) => {
                self.response_text(response_id, offset).map(Value::Object)
            }
            NativeFunctionKind::ResponseHeaderGet(response_id) => {
                let Some(Value::String(name)) = arguments.first() else {
                    return Ok(Value::Null);
                };
                let name = name.to_ascii_lowercase();
                Ok(self
                    .fetch_responses
                    .get(&response_id)
                    .and_then(|response| response.headers.get(&name))
                    .map_or(Value::Null, |value| Value::String(value.clone())))
            }
            NativeFunctionKind::ResponseHeaderHas(response_id) => {
                let name = arguments
                    .first()
                    .and_then(Value::as_str)
                    .unwrap_or_default()
                    .to_ascii_lowercase();
                Ok(Value::Boolean(
                    self.fetch_responses
                        .get(&response_id)
                        .is_some_and(|response| response.headers.contains_key(&name)),
                ))
            }
            // PHANTOM_2M16_WEB_GLOBALS_CORE_BUILTINS_I
            NativeFunctionKind::ConsoleNoop => Ok(Value::Undefined),
            NativeFunctionKind::MathAbs => Ok(Value::Number(
                arguments.first().map(to_number).unwrap_or(f64::NAN).abs(),
            )),
            NativeFunctionKind::MathFloor => Ok(Value::Number(
                arguments.first().map(to_number).unwrap_or(f64::NAN).floor(),
            )),
            NativeFunctionKind::MathCeil => Ok(Value::Number(
                arguments.first().map(to_number).unwrap_or(f64::NAN).ceil(),
            )),
            NativeFunctionKind::MathRound => Ok(Value::Number(math_round_compat(
                arguments.first().map(to_number).unwrap_or(f64::NAN),
            ))),
            NativeFunctionKind::MathMin => Ok(Value::Number(math_min_compat(&arguments))),
            NativeFunctionKind::MathMax => Ok(Value::Number(math_max_compat(&arguments))),
            NativeFunctionKind::MathPow => Ok(Value::Number(
                arguments
                    .first()
                    .map(to_number)
                    .unwrap_or(f64::NAN)
                    .powf(arguments.get(1).map(to_number).unwrap_or(f64::NAN)),
            )),
            NativeFunctionKind::MathSqrt => Ok(Value::Number(
                arguments.first().map(to_number).unwrap_or(f64::NAN).sqrt(),
            )),
            NativeFunctionKind::ArrayIsArray => Ok(Value::Boolean(matches!(
                arguments.first(),
                Some(Value::Array(_))
            ))),
            NativeFunctionKind::NumberIsFinite => Ok(Value::Boolean(matches!(
                arguments.first(),
                Some(Value::Number(value)) if value.is_finite()
            ))),
            NativeFunctionKind::ParseInt => Ok(Value::Number(parse_int_compat(&arguments))),
            NativeFunctionKind::ParseFloat => Ok(Value::Number(parse_float_compat(&arguments))),
            NativeFunctionKind::GlobalIsNaN => Ok(Value::Boolean(
                arguments
                    .first()
                    .map(to_number)
                    .unwrap_or(f64::NAN)
                    .is_nan(),
            )),
            // PHANTOM_2M19_WEB_CORE_LIBRARY_FINAL_GATE
            NativeFunctionKind::MathTrunc => Ok(Value::Number(
                arguments.first().map(to_number).unwrap_or(f64::NAN).trunc(),
            )),
            NativeFunctionKind::MathSign => {
                let value = arguments.first().map(to_number).unwrap_or(f64::NAN);
                let sign = if value.is_nan() {
                    f64::NAN
                } else if value == 0.0 {
                    value
                } else if value.is_sign_negative() {
                    -1.0
                } else {
                    1.0
                };
                Ok(Value::Number(sign))
            }
            NativeFunctionKind::ArrayOf => self.allocate_array(arguments, offset),
            NativeFunctionKind::NumberIsInteger => Ok(Value::Boolean(matches!(
                arguments.first(),
                Some(Value::Number(value)) if value.is_finite() && value.fract() == 0.0
            ))),
            NativeFunctionKind::NumberIsNaN => Ok(Value::Boolean(matches!(
                arguments.first(),
                Some(Value::Number(value)) if value.is_nan()
            ))),
            // PHANTOM_2M17_WEB_CORE_LIBRARY_II
            NativeFunctionKind::ObjectKeys => self.object_keys(&arguments, offset),
            NativeFunctionKind::ObjectValues => self.object_values(&arguments, offset),
            NativeFunctionKind::ObjectAssign => self.object_assign(&arguments, offset),
            NativeFunctionKind::JsonParse => self.json_parse(&arguments, offset),
            NativeFunctionKind::JsonStringify => self.json_stringify(&arguments, offset),
            NativeFunctionKind::ArrayPush => self.array_push(this_value, arguments, offset),
            NativeFunctionKind::ArrayPop => self.array_pop(this_value, offset),
            NativeFunctionKind::ArrayJoin => self.array_join(this_value, &arguments, offset),
            NativeFunctionKind::ArrayIndexOf => {
                self.array_index_of(this_value, &arguments, false, offset)
            }
            NativeFunctionKind::ArrayIncludes => {
                self.array_index_of(this_value, &arguments, true, offset)
            }
            NativeFunctionKind::ArraySlice => self.array_slice(this_value, &arguments, offset),
            NativeFunctionKind::StringTrim => self.string_trim(this_value, offset),
            NativeFunctionKind::StringToLowerCase => self.string_case(this_value, false, offset),
            NativeFunctionKind::StringToUpperCase => self.string_case(this_value, true, offset),
            NativeFunctionKind::StringIncludes => {
                self.string_search(this_value, &arguments, StringSearchKind::Includes, offset)
            }
            NativeFunctionKind::StringStartsWith => {
                self.string_search(this_value, &arguments, StringSearchKind::StartsWith, offset)
            }
            NativeFunctionKind::StringEndsWith => {
                self.string_search(this_value, &arguments, StringSearchKind::EndsWith, offset)
            }
            NativeFunctionKind::StringIndexOf => {
                self.string_search(this_value, &arguments, StringSearchKind::IndexOf, offset)
            }
            NativeFunctionKind::StringSlice => self.string_slice(this_value, &arguments, offset),
            // PHANTOM_2M18_WEB_CORE_LIBRARY_III
            NativeFunctionKind::ObjectEntries => self.object_entries(&arguments, offset),
            NativeFunctionKind::ObjectHasOwn => self.object_has_own(&arguments, offset),
            NativeFunctionKind::ArrayForEach => self.array_for_each(this_value, &arguments, offset),
            NativeFunctionKind::ArrayMap => self.array_map(this_value, &arguments, offset),
            NativeFunctionKind::ArrayFilter => self.array_filter(this_value, &arguments, offset),
            NativeFunctionKind::ArraySome => self.array_some(this_value, &arguments, offset),
            NativeFunctionKind::ArrayEvery => self.array_every(this_value, &arguments, offset),
            NativeFunctionKind::ArrayFind => self.array_find(this_value, &arguments, offset),
            NativeFunctionKind::ArrayFindIndex => {
                self.array_find_index(this_value, &arguments, offset)
            }
            NativeFunctionKind::ArrayReduce => self.array_reduce(this_value, &arguments, offset),
            NativeFunctionKind::StringSplit => self.string_split(this_value, &arguments, offset),
            NativeFunctionKind::StringReplace => {
                self.string_replace(this_value, &arguments, offset)
            }
            // PHANTOM_2M19_WEB_CORE_LIBRARY_FINAL_GATE
            NativeFunctionKind::ObjectFromEntries => self.object_from_entries(&arguments, offset),
            NativeFunctionKind::ArrayAt => self.array_at(this_value, &arguments, offset),
            NativeFunctionKind::ArrayShift => self.array_shift(this_value, offset),
            NativeFunctionKind::ArrayUnshift => self.array_unshift(this_value, arguments, offset),
            NativeFunctionKind::ArrayConcat => self.array_concat(this_value, &arguments, offset),
            NativeFunctionKind::ArrayReverse => self.array_reverse(this_value, offset),
            NativeFunctionKind::StringCharAt => self.string_char_at(this_value, &arguments, offset),
            NativeFunctionKind::StringSubstring => {
                self.string_substring(this_value, &arguments, offset)
            }
            NativeFunctionKind::StringRepeat => self.string_repeat(this_value, &arguments, offset),
            NativeFunctionKind::StringPadStart => {
                self.string_pad(this_value, &arguments, true, offset)
            }
            NativeFunctionKind::StringPadEnd => {
                self.string_pad(this_value, &arguments, false, offset)
            }
            NativeFunctionKind::StringReplaceAll => {
                self.string_replace_all(this_value, &arguments, offset)
            }
        }
    }

    // PHANTOM_2M19_WEB_CORE_LIBRARY_FINAL_GATE
    fn object_from_entries(
        &mut self,
        arguments: &[Value],
        offset: usize,
    ) -> Result<Value, RuntimeError> {
        let Some(Value::Array(entries_id)) = arguments.first() else {
            return Err(RuntimeError::new(
                RuntimeErrorKind::InvalidMemberTarget,
                offset,
            ));
        };
        let Some(entries) = self.arrays.get(*entries_id).cloned() else {
            return Err(RuntimeError::new(
                RuntimeErrorKind::InvalidMemberTarget,
                offset,
            ));
        };
        let mut object = BTreeMap::new();
        for entry in entries {
            let Value::Array(pair_id) = entry else {
                return Err(RuntimeError::new(
                    RuntimeErrorKind::InvalidMemberTarget,
                    offset,
                ));
            };
            let Some(pair) = self.arrays.get(pair_id) else {
                return Err(RuntimeError::new(
                    RuntimeErrorKind::InvalidMemberTarget,
                    offset,
                ));
            };
            let key = pair
                .first()
                .map(runtime_string)
                .unwrap_or_else(|| "undefined".to_owned());
            let value = pair.get(1).cloned().unwrap_or(Value::Undefined);
            if !object.contains_key(&key) && object.len() >= MAX_OBJECT_PROPERTIES {
                return Err(RuntimeError::new(
                    RuntimeErrorKind::PropertyLimitExceeded,
                    offset,
                ));
            }
            object.insert(key, value);
        }
        self.allocate_object(object, offset)
    }

    fn array_at(
        &self,
        this_value: Value,
        arguments: &[Value],
        offset: usize,
    ) -> Result<Value, RuntimeError> {
        let Value::Array(array_id) = this_value else {
            return Err(RuntimeError::new(
                RuntimeErrorKind::InvalidMemberTarget,
                offset,
            ));
        };
        let Some(array) = self.arrays.get(array_id) else {
            return Err(RuntimeError::new(
                RuntimeErrorKind::InvalidMemberTarget,
                offset,
            ));
        };
        let number = arguments.first().map(to_number).unwrap_or(0.0);
        if number.is_nan() {
            return Ok(array.first().cloned().unwrap_or(Value::Undefined));
        }
        if !number.is_finite() {
            return Ok(Value::Undefined);
        }
        let integer = number.trunc();
        let index = if integer < 0.0 {
            let distance = (-integer) as usize;
            if distance > array.len() {
                return Ok(Value::Undefined);
            }
            array.len().saturating_sub(distance)
        } else {
            integer as usize
        };
        Ok(array.get(index).cloned().unwrap_or(Value::Undefined))
    }

    fn array_shift(&mut self, this_value: Value, offset: usize) -> Result<Value, RuntimeError> {
        let Value::Array(array_id) = this_value else {
            return Err(RuntimeError::new(
                RuntimeErrorKind::InvalidMemberTarget,
                offset,
            ));
        };
        let Some(array) = self.arrays.get_mut(array_id) else {
            return Err(RuntimeError::new(
                RuntimeErrorKind::InvalidMemberTarget,
                offset,
            ));
        };
        if array.is_empty() {
            Ok(Value::Undefined)
        } else {
            Ok(array.remove(0))
        }
    }

    fn array_unshift(
        &mut self,
        this_value: Value,
        arguments: Vec<Value>,
        offset: usize,
    ) -> Result<Value, RuntimeError> {
        let Value::Array(array_id) = this_value else {
            return Err(RuntimeError::new(
                RuntimeErrorKind::InvalidMemberTarget,
                offset,
            ));
        };
        let Some(array) = self.arrays.get_mut(array_id) else {
            return Err(RuntimeError::new(
                RuntimeErrorKind::InvalidMemberTarget,
                offset,
            ));
        };
        if array.len().saturating_add(arguments.len()) > MAX_ARRAY_LENGTH {
            return Err(RuntimeError::new(
                RuntimeErrorKind::ArrayLimitExceeded,
                offset,
            ));
        }
        array.splice(0..0, arguments);
        Ok(Value::Number(array.len() as f64))
    }

    fn array_concat(
        &mut self,
        this_value: Value,
        arguments: &[Value],
        offset: usize,
    ) -> Result<Value, RuntimeError> {
        let Value::Array(array_id) = this_value else {
            return Err(RuntimeError::new(
                RuntimeErrorKind::InvalidMemberTarget,
                offset,
            ));
        };
        let Some(array) = self.arrays.get(array_id).cloned() else {
            return Err(RuntimeError::new(
                RuntimeErrorKind::InvalidMemberTarget,
                offset,
            ));
        };
        let mut values = array;
        for argument in arguments {
            match argument {
                Value::Array(other_id) => {
                    let Some(other) = self.arrays.get(*other_id) else {
                        return Err(RuntimeError::new(
                            RuntimeErrorKind::InvalidMemberTarget,
                            offset,
                        ));
                    };
                    if values.len().saturating_add(other.len()) > MAX_ARRAY_LENGTH {
                        return Err(RuntimeError::new(
                            RuntimeErrorKind::ArrayLimitExceeded,
                            offset,
                        ));
                    }
                    values.extend(other.iter().cloned());
                }
                value => {
                    if values.len() >= MAX_ARRAY_LENGTH {
                        return Err(RuntimeError::new(
                            RuntimeErrorKind::ArrayLimitExceeded,
                            offset,
                        ));
                    }
                    values.push(value.clone());
                }
            }
        }
        self.allocate_array(values, offset)
    }

    fn array_reverse(&mut self, this_value: Value, offset: usize) -> Result<Value, RuntimeError> {
        let Value::Array(array_id) = this_value else {
            return Err(RuntimeError::new(
                RuntimeErrorKind::InvalidMemberTarget,
                offset,
            ));
        };
        let Some(array) = self.arrays.get_mut(array_id) else {
            return Err(RuntimeError::new(
                RuntimeErrorKind::InvalidMemberTarget,
                offset,
            ));
        };
        array.reverse();
        Ok(Value::Array(array_id))
    }

    fn string_char_at(
        &self,
        this_value: Value,
        arguments: &[Value],
        offset: usize,
    ) -> Result<Value, RuntimeError> {
        let value = Self::string_receiver(this_value, offset)?;
        let length = value.chars().count();
        let number = arguments.first().map(to_number).unwrap_or(0.0);
        let integer = if number.is_nan() { 0.0 } else { number.trunc() };
        if !integer.is_finite() || integer < 0.0 || integer >= length as f64 {
            return Ok(Value::String(String::new()));
        }
        Ok(Value::String(
            value
                .chars()
                .nth(integer as usize)
                .map_or_else(String::new, |character| character.to_string()),
        ))
    }

    fn string_substring(
        &self,
        this_value: Value,
        arguments: &[Value],
        offset: usize,
    ) -> Result<Value, RuntimeError> {
        let value = Self::string_receiver(this_value, offset)?;
        let chars = value.chars().collect::<Vec<_>>();
        let mut start = bounded_position(arguments.first(), chars.len(), false);
        let mut end = bounded_position(arguments.get(1), chars.len(), true);
        if start > end {
            std::mem::swap(&mut start, &mut end);
        }
        Ok(Value::String(chars[start..end].iter().collect()))
    }

    fn string_repeat(
        &self,
        this_value: Value,
        arguments: &[Value],
        offset: usize,
    ) -> Result<Value, RuntimeError> {
        let value = Self::string_receiver(this_value, offset)?;
        let number = arguments.first().map(to_number).unwrap_or(0.0);
        if number.is_infinite() || number < 0.0 {
            return Err(RuntimeError::new(
                RuntimeErrorKind::ArgumentLimitExceeded,
                offset,
            ));
        }
        let count = if number.is_nan() {
            0
        } else {
            number.trunc() as usize
        };
        if value.len().saturating_mul(count) > MAX_RUNTIME_STRING_BYTES {
            return Err(RuntimeError::new(
                RuntimeErrorKind::StringLimitExceeded,
                offset,
            ));
        }
        Ok(Value::String(value.repeat(count)))
    }

    fn string_pad(
        &self,
        this_value: Value,
        arguments: &[Value],
        start: bool,
        offset: usize,
    ) -> Result<Value, RuntimeError> {
        let value = Self::string_receiver(this_value, offset)?;
        let value_chars = value.chars().count();
        let target = bounded_position(arguments.first(), MAX_RUNTIME_STRING_BYTES, false);
        if target <= value_chars {
            return Ok(Value::String(value));
        }
        let pad = match arguments.get(1) {
            None | Some(Value::Undefined) => " ".to_owned(),
            Some(value) => runtime_string(value),
        };
        if pad.is_empty() {
            return Ok(Value::String(value));
        }
        let needed = target.saturating_sub(value_chars);
        let pad_chars = pad.chars().collect::<Vec<_>>();
        let mut fill = String::new();
        for index in 0..needed {
            fill.push(pad_chars[index % pad_chars.len()]);
            if fill.len().saturating_add(value.len()) > MAX_RUNTIME_STRING_BYTES {
                return Err(RuntimeError::new(
                    RuntimeErrorKind::StringLimitExceeded,
                    offset,
                ));
            }
        }
        let output = if start {
            format!("{fill}{value}")
        } else {
            format!("{value}{fill}")
        };
        if output.len() > MAX_RUNTIME_STRING_BYTES {
            return Err(RuntimeError::new(
                RuntimeErrorKind::StringLimitExceeded,
                offset,
            ));
        }
        Ok(Value::String(output))
    }

    fn string_replace_all(
        &self,
        this_value: Value,
        arguments: &[Value],
        offset: usize,
    ) -> Result<Value, RuntimeError> {
        let value = Self::string_receiver(this_value, offset)?;
        let search = arguments
            .first()
            .map(runtime_string)
            .unwrap_or_else(|| "undefined".to_owned());
        let replacement = arguments
            .get(1)
            .map(runtime_string)
            .unwrap_or_else(|| "undefined".to_owned());
        let mut output = String::new();
        if search.is_empty() {
            output.push_str(&replacement);
            if output.len() > MAX_RUNTIME_STRING_BYTES {
                return Err(RuntimeError::new(
                    RuntimeErrorKind::StringLimitExceeded,
                    offset,
                ));
            }
            for character in value.chars() {
                output.push(character);
                output.push_str(&replacement);
                if output.len() > MAX_RUNTIME_STRING_BYTES {
                    return Err(RuntimeError::new(
                        RuntimeErrorKind::StringLimitExceeded,
                        offset,
                    ));
                }
            }
        } else {
            let mut remaining = value.as_str();
            while let Some(index) = remaining.find(&search) {
                output.push_str(&remaining[..index]);
                output.push_str(&replacement);
                if output.len() > MAX_RUNTIME_STRING_BYTES {
                    return Err(RuntimeError::new(
                        RuntimeErrorKind::StringLimitExceeded,
                        offset,
                    ));
                }
                remaining = &remaining[index + search.len()..];
            }
            output.push_str(remaining);
        }
        if output.len() > MAX_RUNTIME_STRING_BYTES {
            return Err(RuntimeError::new(
                RuntimeErrorKind::StringLimitExceeded,
                offset,
            ));
        }
        Ok(Value::String(output))
    }

    // PHANTOM_2M18_WEB_CORE_LIBRARY_III
    fn object_entries(
        &mut self,
        arguments: &[Value],
        offset: usize,
    ) -> Result<Value, RuntimeError> {
        let Some(Value::Object(object_id)) = arguments.first() else {
            return Err(RuntimeError::new(
                RuntimeErrorKind::InvalidMemberTarget,
                offset,
            ));
        };
        let Some(object) = self.objects.get(*object_id).cloned() else {
            return Err(RuntimeError::new(
                RuntimeErrorKind::InvalidMemberTarget,
                offset,
            ));
        };
        let mut entries = Vec::with_capacity(object.len());
        for (key, value) in object {
            entries.push(self.allocate_array(vec![Value::String(key), value], offset)?);
        }
        self.allocate_array(entries, offset)
    }

    fn object_has_own(&self, arguments: &[Value], offset: usize) -> Result<Value, RuntimeError> {
        let Some(Value::Object(object_id)) = arguments.first() else {
            return Err(RuntimeError::new(
                RuntimeErrorKind::InvalidMemberTarget,
                offset,
            ));
        };
        let Some(object) = self.objects.get(*object_id) else {
            return Err(RuntimeError::new(
                RuntimeErrorKind::InvalidMemberTarget,
                offset,
            ));
        };
        let key = arguments
            .get(1)
            .map(runtime_string)
            .unwrap_or_else(|| "undefined".to_owned());
        Ok(Value::Boolean(object.contains_key(&key)))
    }

    fn array_callback_snapshot(
        &self,
        this_value: Value,
        arguments: &[Value],
        offset: usize,
    ) -> Result<(usize, Vec<Value>, Value), RuntimeError> {
        let Value::Array(array_id) = this_value else {
            return Err(RuntimeError::new(
                RuntimeErrorKind::InvalidMemberTarget,
                offset,
            ));
        };
        let callback = arguments.first().cloned().unwrap_or(Value::Undefined);
        if !is_callable(&callback) {
            return Err(RuntimeError::new(RuntimeErrorKind::CallNonFunction, offset));
        }
        let Some(snapshot) = self.arrays.get(array_id).cloned() else {
            return Err(RuntimeError::new(
                RuntimeErrorKind::InvalidMemberTarget,
                offset,
            ));
        };
        Ok((array_id, snapshot, callback))
    }

    fn array_for_each(
        &mut self,
        this_value: Value,
        arguments: &[Value],
        offset: usize,
    ) -> Result<Value, RuntimeError> {
        let (array_id, snapshot, callback) =
            self.array_callback_snapshot(this_value, arguments, offset)?;
        let this_arg = arguments.get(1).cloned().unwrap_or(Value::Undefined);
        for (index, value) in snapshot.into_iter().enumerate() {
            self.call_value_with_this(
                callback.clone(),
                vec![value, Value::Number(index as f64), Value::Array(array_id)],
                this_arg.clone(),
                offset,
            )?;
        }
        Ok(Value::Undefined)
    }

    fn array_map(
        &mut self,
        this_value: Value,
        arguments: &[Value],
        offset: usize,
    ) -> Result<Value, RuntimeError> {
        let (array_id, snapshot, callback) =
            self.array_callback_snapshot(this_value, arguments, offset)?;
        let this_arg = arguments.get(1).cloned().unwrap_or(Value::Undefined);
        let mut mapped = Vec::with_capacity(snapshot.len());
        for (index, value) in snapshot.into_iter().enumerate() {
            mapped.push(self.call_value_with_this(
                callback.clone(),
                vec![value, Value::Number(index as f64), Value::Array(array_id)],
                this_arg.clone(),
                offset,
            )?);
        }
        self.allocate_array(mapped, offset)
    }

    fn array_filter(
        &mut self,
        this_value: Value,
        arguments: &[Value],
        offset: usize,
    ) -> Result<Value, RuntimeError> {
        let (array_id, snapshot, callback) =
            self.array_callback_snapshot(this_value, arguments, offset)?;
        let this_arg = arguments.get(1).cloned().unwrap_or(Value::Undefined);
        let mut filtered = Vec::new();
        for (index, value) in snapshot.into_iter().enumerate() {
            let keep = self.call_value_with_this(
                callback.clone(),
                vec![
                    value.clone(),
                    Value::Number(index as f64),
                    Value::Array(array_id),
                ],
                this_arg.clone(),
                offset,
            )?;
            if is_truthy(&keep) {
                filtered.push(value);
            }
        }
        self.allocate_array(filtered, offset)
    }

    fn array_some(
        &mut self,
        this_value: Value,
        arguments: &[Value],
        offset: usize,
    ) -> Result<Value, RuntimeError> {
        let (array_id, snapshot, callback) =
            self.array_callback_snapshot(this_value, arguments, offset)?;
        let this_arg = arguments.get(1).cloned().unwrap_or(Value::Undefined);
        for (index, value) in snapshot.into_iter().enumerate() {
            let result = self.call_value_with_this(
                callback.clone(),
                vec![value, Value::Number(index as f64), Value::Array(array_id)],
                this_arg.clone(),
                offset,
            )?;
            if is_truthy(&result) {
                return Ok(Value::Boolean(true));
            }
        }
        Ok(Value::Boolean(false))
    }

    fn array_every(
        &mut self,
        this_value: Value,
        arguments: &[Value],
        offset: usize,
    ) -> Result<Value, RuntimeError> {
        let (array_id, snapshot, callback) =
            self.array_callback_snapshot(this_value, arguments, offset)?;
        let this_arg = arguments.get(1).cloned().unwrap_or(Value::Undefined);
        for (index, value) in snapshot.into_iter().enumerate() {
            let result = self.call_value_with_this(
                callback.clone(),
                vec![value, Value::Number(index as f64), Value::Array(array_id)],
                this_arg.clone(),
                offset,
            )?;
            if !is_truthy(&result) {
                return Ok(Value::Boolean(false));
            }
        }
        Ok(Value::Boolean(true))
    }

    fn array_find(
        &mut self,
        this_value: Value,
        arguments: &[Value],
        offset: usize,
    ) -> Result<Value, RuntimeError> {
        let (array_id, snapshot, callback) =
            self.array_callback_snapshot(this_value, arguments, offset)?;
        let this_arg = arguments.get(1).cloned().unwrap_or(Value::Undefined);
        for (index, value) in snapshot.into_iter().enumerate() {
            let result = self.call_value_with_this(
                callback.clone(),
                vec![
                    value.clone(),
                    Value::Number(index as f64),
                    Value::Array(array_id),
                ],
                this_arg.clone(),
                offset,
            )?;
            if is_truthy(&result) {
                return Ok(value);
            }
        }
        Ok(Value::Undefined)
    }

    fn array_find_index(
        &mut self,
        this_value: Value,
        arguments: &[Value],
        offset: usize,
    ) -> Result<Value, RuntimeError> {
        let (array_id, snapshot, callback) =
            self.array_callback_snapshot(this_value, arguments, offset)?;
        let this_arg = arguments.get(1).cloned().unwrap_or(Value::Undefined);
        for (index, value) in snapshot.into_iter().enumerate() {
            let result = self.call_value_with_this(
                callback.clone(),
                vec![value, Value::Number(index as f64), Value::Array(array_id)],
                this_arg.clone(),
                offset,
            )?;
            if is_truthy(&result) {
                return Ok(Value::Number(index as f64));
            }
        }
        Ok(Value::Number(-1.0))
    }

    fn array_reduce(
        &mut self,
        this_value: Value,
        arguments: &[Value],
        offset: usize,
    ) -> Result<Value, RuntimeError> {
        let (array_id, snapshot, callback) =
            self.array_callback_snapshot(this_value, arguments, offset)?;
        let (mut accumulator, start) = if let Some(initial) = arguments.get(1) {
            (initial.clone(), 0usize)
        } else if let Some(first) = snapshot.first() {
            (first.clone(), 1usize)
        } else {
            return Err(RuntimeError::new(
                RuntimeErrorKind::ReduceEmptyArray,
                offset,
            ));
        };
        for (index, value) in snapshot.into_iter().enumerate().skip(start) {
            accumulator = self.call_value_with_this(
                callback.clone(),
                vec![
                    accumulator,
                    value,
                    Value::Number(index as f64),
                    Value::Array(array_id),
                ],
                Value::Undefined,
                offset,
            )?;
        }
        Ok(accumulator)
    }

    fn string_split(
        &mut self,
        this_value: Value,
        arguments: &[Value],
        offset: usize,
    ) -> Result<Value, RuntimeError> {
        let value = Self::string_receiver(this_value, offset)?;
        let limit = match arguments.get(1) {
            None | Some(Value::Undefined) => MAX_ARRAY_LENGTH,
            Some(raw) => {
                let number = to_number(raw);
                if !number.is_finite() {
                    0
                } else if number <= 0.0 {
                    0
                } else {
                    (number.trunc() as usize).min(MAX_ARRAY_LENGTH)
                }
            }
        };
        if limit == 0 {
            return self.allocate_array(Vec::new(), offset);
        }
        let parts = match arguments.first() {
            None | Some(Value::Undefined) => vec![Value::String(value)],
            Some(separator) => {
                let separator = runtime_string(separator);
                if separator.is_empty() {
                    value
                        .chars()
                        .take(limit)
                        .map(|character| Value::String(character.to_string()))
                        .collect::<Vec<_>>()
                } else {
                    value
                        .split(&separator)
                        .take(limit)
                        .map(|part| Value::String(part.to_owned()))
                        .collect::<Vec<_>>()
                }
            }
        };
        self.allocate_array(parts, offset)
    }

    fn string_replace(
        &self,
        this_value: Value,
        arguments: &[Value],
        offset: usize,
    ) -> Result<Value, RuntimeError> {
        let value = Self::string_receiver(this_value, offset)?;
        let search = arguments
            .first()
            .map(runtime_string)
            .unwrap_or_else(|| "undefined".to_owned());
        let replacement = arguments
            .get(1)
            .map(runtime_string)
            .unwrap_or_else(|| "undefined".to_owned());
        let output = if let Some(index) = value.find(&search) {
            let mut output = String::with_capacity(
                value
                    .len()
                    .saturating_sub(search.len())
                    .saturating_add(replacement.len()),
            );
            output.push_str(&value[..index]);
            output.push_str(&replacement);
            output.push_str(&value[index + search.len()..]);
            output
        } else {
            value
        };
        if output.len() > MAX_RUNTIME_STRING_BYTES {
            return Err(RuntimeError::new(
                RuntimeErrorKind::StringLimitExceeded,
                offset,
            ));
        }
        Ok(Value::String(output))
    }

    // PHANTOM_2M17_WEB_CORE_LIBRARY_II
    fn object_keys(&mut self, arguments: &[Value], offset: usize) -> Result<Value, RuntimeError> {
        let Some(Value::Object(object_id)) = arguments.first() else {
            return Err(RuntimeError::new(
                RuntimeErrorKind::InvalidMemberTarget,
                offset,
            ));
        };
        let Some(object) = self.objects.get(*object_id) else {
            return Err(RuntimeError::new(
                RuntimeErrorKind::InvalidMemberTarget,
                offset,
            ));
        };
        let values = object
            .keys()
            .cloned()
            .map(Value::String)
            .collect::<Vec<_>>();
        self.allocate_array(values, offset)
    }

    fn object_values(&mut self, arguments: &[Value], offset: usize) -> Result<Value, RuntimeError> {
        let Some(Value::Object(object_id)) = arguments.first() else {
            return Err(RuntimeError::new(
                RuntimeErrorKind::InvalidMemberTarget,
                offset,
            ));
        };
        let Some(object) = self.objects.get(*object_id) else {
            return Err(RuntimeError::new(
                RuntimeErrorKind::InvalidMemberTarget,
                offset,
            ));
        };
        let values = object.values().cloned().collect::<Vec<_>>();
        self.allocate_array(values, offset)
    }

    fn object_assign(&mut self, arguments: &[Value], offset: usize) -> Result<Value, RuntimeError> {
        let Some(Value::Object(target_id)) = arguments.first().cloned() else {
            return Err(RuntimeError::new(
                RuntimeErrorKind::InvalidMemberTarget,
                offset,
            ));
        };
        let mut additions = Vec::new();
        for source in arguments.iter().skip(1) {
            match source {
                Value::Null | Value::Undefined => continue,
                Value::Object(source_id) => {
                    let Some(source_object) = self.objects.get(*source_id) else {
                        return Err(RuntimeError::new(
                            RuntimeErrorKind::InvalidMemberTarget,
                            offset,
                        ));
                    };
                    additions.extend(
                        source_object
                            .iter()
                            .map(|(key, value)| (key.clone(), value.clone())),
                    );
                }
                _ => {
                    return Err(RuntimeError::new(
                        RuntimeErrorKind::InvalidMemberTarget,
                        offset,
                    ));
                }
            }
        }
        let Some(target) = self.objects.get_mut(target_id) else {
            return Err(RuntimeError::new(
                RuntimeErrorKind::InvalidMemberTarget,
                offset,
            ));
        };
        let additional_keys = additions
            .iter()
            .filter(|(key, _)| !target.contains_key(key))
            .count();
        if target.len().saturating_add(additional_keys) > MAX_OBJECT_PROPERTIES {
            return Err(RuntimeError::new(
                RuntimeErrorKind::PropertyLimitExceeded,
                offset,
            ));
        }
        for (key, value) in additions {
            target.insert(key, value);
        }
        Ok(Value::Object(target_id))
    }

    fn array_push(
        &mut self,
        this_value: Value,
        arguments: Vec<Value>,
        offset: usize,
    ) -> Result<Value, RuntimeError> {
        let Value::Array(array_id) = this_value else {
            return Err(RuntimeError::new(
                RuntimeErrorKind::InvalidMemberTarget,
                offset,
            ));
        };
        let Some(array) = self.arrays.get_mut(array_id) else {
            return Err(RuntimeError::new(
                RuntimeErrorKind::InvalidMemberTarget,
                offset,
            ));
        };
        if array.len().saturating_add(arguments.len()) > MAX_ARRAY_LENGTH {
            return Err(RuntimeError::new(
                RuntimeErrorKind::ArrayLimitExceeded,
                offset,
            ));
        }
        array.extend(arguments);
        Ok(Value::Number(array.len() as f64))
    }

    fn array_pop(&mut self, this_value: Value, offset: usize) -> Result<Value, RuntimeError> {
        let Value::Array(array_id) = this_value else {
            return Err(RuntimeError::new(
                RuntimeErrorKind::InvalidMemberTarget,
                offset,
            ));
        };
        let Some(array) = self.arrays.get_mut(array_id) else {
            return Err(RuntimeError::new(
                RuntimeErrorKind::InvalidMemberTarget,
                offset,
            ));
        };
        Ok(array.pop().unwrap_or(Value::Undefined))
    }

    fn array_join(
        &mut self,
        this_value: Value,
        arguments: &[Value],
        offset: usize,
    ) -> Result<Value, RuntimeError> {
        let Value::Array(array_id) = this_value else {
            return Err(RuntimeError::new(
                RuntimeErrorKind::InvalidMemberTarget,
                offset,
            ));
        };
        let Some(array) = self.arrays.get(array_id).cloned() else {
            return Err(RuntimeError::new(
                RuntimeErrorKind::InvalidMemberTarget,
                offset,
            ));
        };
        let separator = match arguments.first() {
            None | Some(Value::Undefined) => ",".to_owned(),
            Some(value) => runtime_string(value),
        };
        let mut output = String::new();
        for (index, value) in array.iter().enumerate() {
            if index > 0 {
                output.push_str(&separator);
            }
            if !matches!(value, Value::Undefined | Value::Null) {
                output.push_str(&runtime_string(value));
            }
            if output.len() > MAX_RUNTIME_STRING_BYTES {
                return Err(RuntimeError::new(
                    RuntimeErrorKind::StringLimitExceeded,
                    offset,
                ));
            }
        }
        Ok(Value::String(output))
    }

    fn array_index_of(
        &mut self,
        this_value: Value,
        arguments: &[Value],
        same_value_zero: bool,
        offset: usize,
    ) -> Result<Value, RuntimeError> {
        let Value::Array(array_id) = this_value else {
            return Err(RuntimeError::new(
                RuntimeErrorKind::InvalidMemberTarget,
                offset,
            ));
        };
        let Some(array) = self.arrays.get(array_id) else {
            return Err(RuntimeError::new(
                RuntimeErrorKind::InvalidMemberTarget,
                offset,
            ));
        };
        let needle = arguments.first().cloned().unwrap_or(Value::Undefined);
        let start = relative_index(arguments.get(1), array.len(), false);
        let found = array.iter().enumerate().skip(start).find_map(|(index, value)| {
            let equal = strict_equal(value, &needle)
                || (same_value_zero
                    && matches!((value, &needle), (Value::Number(left), Value::Number(right)) if left.is_nan() && right.is_nan()));
            equal.then_some(index)
        });
        if same_value_zero {
            Ok(Value::Boolean(found.is_some()))
        } else {
            Ok(Value::Number(found.map_or(-1.0, |index| index as f64)))
        }
    }

    fn array_slice(
        &mut self,
        this_value: Value,
        arguments: &[Value],
        offset: usize,
    ) -> Result<Value, RuntimeError> {
        let Value::Array(array_id) = this_value else {
            return Err(RuntimeError::new(
                RuntimeErrorKind::InvalidMemberTarget,
                offset,
            ));
        };
        let Some(array) = self.arrays.get(array_id).cloned() else {
            return Err(RuntimeError::new(
                RuntimeErrorKind::InvalidMemberTarget,
                offset,
            ));
        };
        let start = relative_index(arguments.first(), array.len(), false);
        let end = relative_index(arguments.get(1), array.len(), true);
        let values = if end <= start {
            Vec::new()
        } else {
            array[start..end].to_vec()
        };
        self.allocate_array(values, offset)
    }

    fn string_receiver(this_value: Value, offset: usize) -> Result<String, RuntimeError> {
        match this_value {
            Value::String(value) => Ok(value),
            _ => Err(RuntimeError::new(
                RuntimeErrorKind::InvalidMemberTarget,
                offset,
            )),
        }
    }

    fn string_trim(&self, this_value: Value, offset: usize) -> Result<Value, RuntimeError> {
        let value = Self::string_receiver(this_value, offset)?;
        Ok(Value::String(value.trim().to_owned()))
    }

    fn string_case(
        &self,
        this_value: Value,
        upper: bool,
        offset: usize,
    ) -> Result<Value, RuntimeError> {
        let value = Self::string_receiver(this_value, offset)?;
        let converted = if upper {
            value.to_uppercase()
        } else {
            value.to_lowercase()
        };
        if converted.len() > MAX_RUNTIME_STRING_BYTES {
            return Err(RuntimeError::new(
                RuntimeErrorKind::StringLimitExceeded,
                offset,
            ));
        }
        Ok(Value::String(converted))
    }

    fn string_search(
        &self,
        this_value: Value,
        arguments: &[Value],
        kind: StringSearchKind,
        offset: usize,
    ) -> Result<Value, RuntimeError> {
        let value = Self::string_receiver(this_value, offset)?;
        let needle = arguments
            .first()
            .map(runtime_string)
            .unwrap_or_else(|| "undefined".to_owned());
        let chars = value.chars().collect::<Vec<_>>();
        let start = bounded_position(arguments.get(1), chars.len(), false);
        let tail = chars.iter().skip(start).collect::<String>();
        match kind {
            StringSearchKind::Includes => Ok(Value::Boolean(tail.contains(&needle))),
            StringSearchKind::StartsWith => Ok(Value::Boolean(tail.starts_with(&needle))),
            StringSearchKind::EndsWith => {
                let end = bounded_position(arguments.get(1), chars.len(), true);
                let head = chars.iter().take(end).collect::<String>();
                Ok(Value::Boolean(head.ends_with(&needle)))
            }
            StringSearchKind::IndexOf => {
                let position = tail
                    .find(&needle)
                    .map(|byte_index| start + tail[..byte_index].chars().count());
                Ok(Value::Number(position.map_or(-1.0, |index| index as f64)))
            }
        }
    }

    fn string_slice(
        &self,
        this_value: Value,
        arguments: &[Value],
        offset: usize,
    ) -> Result<Value, RuntimeError> {
        let value = Self::string_receiver(this_value, offset)?;
        let chars = value.chars().collect::<Vec<_>>();
        let start = relative_index(arguments.first(), chars.len(), false);
        let end = relative_index(arguments.get(1), chars.len(), true);
        let output = if end <= start {
            String::new()
        } else {
            chars[start..end].iter().collect::<String>()
        };
        Ok(Value::String(output))
    }

    fn json_parse(&mut self, arguments: &[Value], offset: usize) -> Result<Value, RuntimeError> {
        let source = arguments
            .first()
            .map(runtime_string)
            .unwrap_or_else(|| "undefined".to_owned());
        if source.len() > MAX_RUNTIME_STRING_BYTES {
            return Err(RuntimeError::new(
                RuntimeErrorKind::StringLimitExceeded,
                offset,
            ));
        }
        let mut parser = JsonParser::new(&source, self, offset);
        parser.parse()
    }

    fn json_stringify(&self, arguments: &[Value], offset: usize) -> Result<Value, RuntimeError> {
        let value = arguments.first().cloned().unwrap_or(Value::Undefined);
        let mut output = String::new();
        let mut seen = BTreeSet::new();
        if !self.json_stringify_value(&value, &mut output, &mut seen, 0, false, offset)? {
            return Ok(Value::Undefined);
        }
        Ok(Value::String(output))
    }

    fn json_stringify_value(
        &self,
        value: &Value,
        output: &mut String,
        seen: &mut BTreeSet<(u8, usize)>,
        depth: usize,
        array_slot: bool,
        offset: usize,
    ) -> Result<bool, RuntimeError> {
        if depth > MAX_JSON_DEPTH {
            return Err(RuntimeError::new(RuntimeErrorKind::InvalidJson, offset));
        }
        match value {
            Value::Undefined
            | Value::Function(_)
            | Value::NativeFunction(_)
            | Value::HostFunction(_) => {
                if array_slot {
                    output.push_str("null");
                    return Ok(true);
                }
                Ok(false)
            }
            Value::Null => {
                output.push_str("null");
                Ok(true)
            }
            Value::Boolean(value) => {
                output.push_str(if *value { "true" } else { "false" });
                Ok(true)
            }
            Value::Number(value) => {
                if value.is_finite() {
                    output.push_str(&runtime_string(&Value::Number(*value)));
                } else {
                    output.push_str("null");
                }
                Ok(true)
            }
            Value::String(value) => {
                append_json_string(output, value);
                Ok(true)
            }
            Value::Array(array_id) => {
                if !seen.insert((0, *array_id)) {
                    return Err(RuntimeError::new(RuntimeErrorKind::InvalidJson, offset));
                }
                let Some(array) = self.arrays.get(*array_id) else {
                    return Err(RuntimeError::new(RuntimeErrorKind::InvalidJson, offset));
                };
                output.push('[');
                for (index, item) in array.iter().enumerate() {
                    if index > 0 {
                        output.push(',');
                    }
                    let _ =
                        self.json_stringify_value(item, output, seen, depth + 1, true, offset)?;
                    if output.len() > MAX_RUNTIME_STRING_BYTES {
                        return Err(RuntimeError::new(
                            RuntimeErrorKind::StringLimitExceeded,
                            offset,
                        ));
                    }
                }
                output.push(']');
                seen.remove(&(0, *array_id));
                Ok(true)
            }
            Value::Object(object_id) => {
                if !seen.insert((1, *object_id)) {
                    return Err(RuntimeError::new(RuntimeErrorKind::InvalidJson, offset));
                }
                let Some(object) = self.objects.get(*object_id) else {
                    return Err(RuntimeError::new(RuntimeErrorKind::InvalidJson, offset));
                };
                output.push('{');
                let mut first = true;
                for (key, item) in object {
                    let mut encoded = String::new();
                    if !self.json_stringify_value(
                        item,
                        &mut encoded,
                        seen,
                        depth + 1,
                        false,
                        offset,
                    )? {
                        continue;
                    }
                    if !first {
                        output.push(',');
                    }
                    first = false;
                    append_json_string(output, key);
                    output.push(':');
                    output.push_str(&encoded);
                    if output.len() > MAX_RUNTIME_STRING_BYTES {
                        return Err(RuntimeError::new(
                            RuntimeErrorKind::StringLimitExceeded,
                            offset,
                        ));
                    }
                }
                output.push('}');
                seen.remove(&(1, *object_id));
                Ok(true)
            }
        }
    }

    // PHANTOM_2N16_FETCH_REQUEST_INIT_HOST_BOUNDARY_I
    fn parse_fetch_request_init(
        &self,
        value: Option<&Value>,
        offset: usize,
    ) -> Result<FetchRequestInit, RuntimeError> {
        let Some(value) = value else {
            return Ok(FetchRequestInit::default());
        };
        if matches!(value, Value::Undefined) {
            return Ok(FetchRequestInit::default());
        }
        let Value::Object(object_id) = value else {
            return Err(RuntimeError::new(
                RuntimeErrorKind::UnsupportedFetchOptions,
                offset,
            ));
        };
        let Some(object) = self.objects.get(*object_id) else {
            return Err(RuntimeError::new(
                RuntimeErrorKind::UnsupportedFetchOptions,
                offset,
            ));
        };

        let method = match object.get("method") {
            None | Some(Value::Undefined) => "GET".to_owned(),
            Some(Value::String(value)) => canonical_fetch_method(value, offset)?,
            _ => {
                return Err(RuntimeError::new(
                    RuntimeErrorKind::UnsupportedFetchOptions,
                    offset,
                ));
            }
        };

        let credentials = match object.get("credentials") {
            None | Some(Value::Undefined) => FetchCredentialsMode::SameOrigin,
            Some(Value::String(value)) if value == "omit" => FetchCredentialsMode::Omit,
            Some(Value::String(value)) if value == "same-origin" => {
                FetchCredentialsMode::SameOrigin
            }
            Some(Value::String(value)) if value == "include" => FetchCredentialsMode::Include,
            _ => {
                return Err(RuntimeError::new(
                    RuntimeErrorKind::UnsupportedFetchOptions,
                    offset,
                ));
            }
        };

        let body = match object.get("body") {
            None | Some(Value::Undefined) | Some(Value::Null) => None,
            Some(Value::String(value)) if value.len() <= MAX_FETCH_BODY_BYTES => {
                Some(value.clone())
            }
            Some(Value::String(_)) => {
                return Err(RuntimeError::new(
                    RuntimeErrorKind::FetchResponseLimitExceeded,
                    offset,
                ));
            }
            _ => {
                return Err(RuntimeError::new(
                    RuntimeErrorKind::UnsupportedFetchOptions,
                    offset,
                ));
            }
        };
        if method == "GET" && body.is_some() {
            return Err(RuntimeError::new(
                RuntimeErrorKind::UnsupportedFetchOptions,
                offset,
            ));
        }

        let headers = match object.get("headers") {
            None | Some(Value::Undefined) => Vec::new(),
            Some(Value::Object(headers_id)) => {
                self.fetch_headers_from_object(*headers_id, offset)?
            }
            _ => {
                return Err(RuntimeError::new(
                    RuntimeErrorKind::UnsupportedFetchOptions,
                    offset,
                ));
            }
        };

        Ok(FetchRequestInit {
            method,
            headers,
            body,
            credentials,
        })
    }

    fn fetch_headers_from_object(
        &self,
        object_id: usize,
        offset: usize,
    ) -> Result<Vec<(String, String)>, RuntimeError> {
        let Some(object) = self.objects.get(object_id) else {
            return Err(RuntimeError::new(
                RuntimeErrorKind::UnsupportedFetchOptions,
                offset,
            ));
        };
        if object.len() > MAX_FETCH_HEADERS {
            return Err(RuntimeError::new(
                RuntimeErrorKind::UnsupportedFetchOptions,
                offset,
            ));
        }
        let mut total = 0usize;
        let mut headers = Vec::with_capacity(object.len());
        for (name, value) in object {
            if name.is_empty()
                || name.len() > MAX_FETCH_HEADER_NAME_BYTES
                || !name
                    .bytes()
                    .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-')
            {
                return Err(RuntimeError::new(
                    RuntimeErrorKind::UnsupportedFetchOptions,
                    offset,
                ));
            }
            let Value::String(value) = value else {
                return Err(RuntimeError::new(
                    RuntimeErrorKind::UnsupportedFetchOptions,
                    offset,
                ));
            };
            if value.len() > MAX_FETCH_HEADER_VALUE_BYTES
                || value.bytes().any(|byte| matches!(byte, b'\r' | b'\n' | 0))
            {
                return Err(RuntimeError::new(
                    RuntimeErrorKind::UnsupportedFetchOptions,
                    offset,
                ));
            }
            total = total.saturating_add(name.len()).saturating_add(value.len());
            if total > MAX_FETCH_HEADER_BYTES {
                return Err(RuntimeError::new(
                    RuntimeErrorKind::UnsupportedFetchOptions,
                    offset,
                ));
            }
            headers.push((name.to_ascii_lowercase(), value.clone()));
        }
        Ok(headers)
    }

    fn issue_fetch(
        &mut self,
        url: String,
        init: FetchRequestInit,
        offset: usize,
    ) -> Result<usize, RuntimeError> {
        if url.is_empty() || url.len() > MAX_FETCH_URL_BYTES {
            return Err(RuntimeError::new(RuntimeErrorKind::InvalidFetchUrl, offset));
        }
        if self.fetch_promises.len() >= MAX_FETCH_REQUESTS {
            return Err(RuntimeError::new(
                RuntimeErrorKind::FetchRequestLimitExceeded,
                offset,
            ));
        }

        let request_id = self.next_fetch_id;
        let Some(next_fetch_id) = self.next_fetch_id.checked_add(1) else {
            return Err(RuntimeError::new(
                RuntimeErrorKind::FetchRequestLimitExceeded,
                offset,
            ));
        };

        let promise = self.allocate_promise(PromiseState::Pending, offset)?;
        self.next_fetch_id = next_fetch_id;
        self.fetch_promises.insert(request_id, promise);
        self.fetch_requests.push_back(FetchRequestRecord {
            id: request_id,
            url,
            method: init.method,
            headers: init.headers,
            body: init.body,
            credentials: init.credentials,
        });
        Ok(promise)
    }

    fn complete_fetch_success_inner(
        &mut self,
        request_id: u64,
        status: u16,
        response_url: String,
        body: String,
    ) -> Result<(), RuntimeError> {
        self.complete_fetch_success_inner_with_headers(
            request_id,
            status,
            response_url,
            body,
            BTreeMap::new(),
        )
    }

    fn complete_fetch_success_inner_with_headers(
        &mut self,
        request_id: u64,
        status: u16,
        response_url: String,
        body: String,
        headers: BTreeMap<String, String>,
    ) -> Result<(), RuntimeError> {
        if !(100..=599).contains(&status) {
            return Err(RuntimeError::new(RuntimeErrorKind::InvalidFetchStatus, 0));
        }
        if response_url.len() > MAX_FETCH_URL_BYTES || body.len() > MAX_FETCH_BODY_BYTES {
            return Err(RuntimeError::new(
                RuntimeErrorKind::FetchResponseLimitExceeded,
                0,
            ));
        }

        let Some(promise_id) = self.fetch_promises.get(&request_id).copied() else {
            return Err(RuntimeError::new(RuntimeErrorKind::UnknownFetchRequest, 0));
        };

        if self.fetch_responses.len() >= MAX_FETCH_RESPONSES {
            return Err(RuntimeError::new(
                RuntimeErrorKind::FetchResponseCountExceeded,
                0,
            ));
        }
        if self.native_functions.len().saturating_add(3) > MAX_NATIVE_FUNCTIONS {
            return Err(RuntimeError::new(
                RuntimeErrorKind::NativeFunctionLimitExceeded,
                0,
            ));
        }

        let response_id = object_id(self.allocate_object(BTreeMap::new(), 0)?, 0)?;
        let text_function =
            self.allocate_native_function(NativeFunctionKind::ResponseText(response_id), 0)?;
        let headers_id = object_id(self.allocate_object(BTreeMap::new(), 0)?, 0)?;
        let get_fn =
            self.allocate_native_function(NativeFunctionKind::ResponseHeaderGet(response_id), 0)?;
        let has_fn =
            self.allocate_native_function(NativeFunctionKind::ResponseHeaderHas(response_id), 0)?;
        self.replace_host_object(
            headers_id,
            BTreeMap::from([
                ("get".to_owned(), Value::NativeFunction(get_fn)),
                ("has".to_owned(), Value::NativeFunction(has_fn)),
            ]),
        )?;
        self.replace_host_object(
            response_id,
            BTreeMap::from([
                ("status".to_owned(), Value::Number(f64::from(status))),
                (
                    "ok".to_owned(),
                    Value::Boolean((200..=299).contains(&status)),
                ),
                ("url".to_owned(), Value::String(response_url)),
                ("bodyUsed".to_owned(), Value::Boolean(false)),
                ("text".to_owned(), Value::NativeFunction(text_function)),
                ("headers".to_owned(), Value::Object(headers_id)),
            ]),
        )?;
        self.fetch_responses.insert(
            response_id,
            FetchResponseRecord {
                body,
                body_used: false,
                headers,
            },
        );

        self.fetch_promises.remove(&request_id);
        self.fetch_requests
            .retain(|request| request.id != request_id);
        self.settle_promise(
            promise_id,
            PromiseState::Fulfilled(Value::Object(response_id)),
            0,
        )?;
        self.perform_microtask_checkpoint_inner()?;
        Ok(())
    }

    fn complete_fetch_failure_inner(
        &mut self,
        request_id: u64,
        reason: String,
    ) -> Result<(), RuntimeError> {
        if reason.len() > MAX_FETCH_FAILURE_BYTES {
            return Err(RuntimeError::new(
                RuntimeErrorKind::FetchFailureLimitExceeded,
                0,
            ));
        }

        let Some(promise_id) = self.fetch_promises.remove(&request_id) else {
            return Err(RuntimeError::new(RuntimeErrorKind::UnknownFetchRequest, 0));
        };
        self.fetch_requests
            .retain(|request| request.id != request_id);
        self.settle_promise(promise_id, PromiseState::Rejected(Value::String(reason)), 0)?;
        self.perform_microtask_checkpoint_inner()?;
        Ok(())
    }

    fn response_text(&mut self, response_id: usize, offset: usize) -> Result<usize, RuntimeError> {
        let Some(response) = self.fetch_responses.get(&response_id) else {
            return Err(RuntimeError::new(
                RuntimeErrorKind::InvalidMemberTarget,
                offset,
            ));
        };

        if response.body_used {
            return self.allocate_promise(
                PromiseState::Rejected(Value::String("response body already consumed".to_owned())),
                offset,
            );
        }

        let body = response.body.clone();
        let promise =
            self.allocate_promise(PromiseState::Fulfilled(Value::String(body)), offset)?;

        let Some(response) = self.fetch_responses.get_mut(&response_id) else {
            return Err(RuntimeError::new(
                RuntimeErrorKind::InvalidMemberTarget,
                offset,
            ));
        };
        response.body_used = true;

        let Some(object) = self.objects.get_mut(response_id) else {
            return Err(RuntimeError::new(
                RuntimeErrorKind::InvalidMemberTarget,
                offset,
            ));
        };
        object.insert("bodyUsed".to_owned(), Value::Boolean(true));
        Ok(promise)
    }

    fn allocate_promise(
        &mut self,
        state: PromiseState,
        offset: usize,
    ) -> Result<usize, RuntimeError> {
        if self.promises.len() >= MAX_PROMISES {
            return Err(RuntimeError::new(
                RuntimeErrorKind::PromiseLimitExceeded,
                offset,
            ));
        }
        if self.objects.len() >= MAX_OBJECTS {
            return Err(RuntimeError::new(
                RuntimeErrorKind::ObjectLimitExceeded,
                offset,
            ));
        }
        if self.native_functions.len().saturating_add(2) > MAX_NATIVE_FUNCTIONS {
            return Err(RuntimeError::new(
                RuntimeErrorKind::NativeFunctionLimitExceeded,
                offset,
            ));
        }

        let object_id = object_id(self.allocate_object(BTreeMap::new(), offset)?, offset)?;
        let then_function =
            self.allocate_native_function(NativeFunctionKind::PromiseThen(object_id), offset)?;
        let catch_function =
            self.allocate_native_function(NativeFunctionKind::PromiseCatch(object_id), offset)?;

        self.replace_host_object(
            object_id,
            BTreeMap::from([
                ("then".to_owned(), Value::NativeFunction(then_function)),
                ("catch".to_owned(), Value::NativeFunction(catch_function)),
            ]),
        )?;
        self.promises.insert(
            object_id,
            PromiseRecord {
                state,
                reactions: Vec::new(),
            },
        );
        Ok(object_id)
    }

    fn register_promise_reaction(
        &mut self,
        promise_id: usize,
        on_fulfilled: Option<Value>,
        on_rejected: Option<Value>,
        offset: usize,
    ) -> Result<usize, RuntimeError> {
        let Some(source) = self.promises.get(&promise_id) else {
            return Err(RuntimeError::new(
                RuntimeErrorKind::InvalidMemberTarget,
                offset,
            ));
        };

        let state = source.state.clone();
        if matches!(state, PromiseState::Pending)
            && source.reactions.len() >= MAX_PROMISE_REACTIONS_PER_PROMISE
        {
            return Err(RuntimeError::new(
                RuntimeErrorKind::PromiseReactionLimitExceeded,
                offset,
            ));
        }
        if !matches!(state, PromiseState::Pending) && self.microtasks.len() >= MAX_MICROTASKS {
            return Err(RuntimeError::new(
                RuntimeErrorKind::MicrotaskLimitExceeded,
                offset,
            ));
        }

        let child = self.allocate_promise(PromiseState::Pending, offset)?;
        let reaction = PromiseReaction {
            on_fulfilled,
            on_rejected,
            child,
        };

        match state {
            PromiseState::Pending => {
                let Some(source) = self.promises.get_mut(&promise_id) else {
                    return Err(RuntimeError::new(
                        RuntimeErrorKind::InvalidMemberTarget,
                        offset,
                    ));
                };
                source.reactions.push(reaction);
            }
            settled => {
                self.enqueue_microtask(
                    Microtask::PromiseReaction {
                        state: settled,
                        reaction,
                    },
                    offset,
                )?;
            }
        }

        Ok(child)
    }

    fn settle_promise(
        &mut self,
        promise_id: usize,
        state: PromiseState,
        offset: usize,
    ) -> Result<(), RuntimeError> {
        if matches!(state, PromiseState::Pending) {
            return Ok(());
        }

        let Some(promise) = self.promises.get(&promise_id) else {
            return Err(RuntimeError::new(
                RuntimeErrorKind::InvalidMemberTarget,
                offset,
            ));
        };
        if !matches!(promise.state, PromiseState::Pending) {
            return Ok(());
        }

        let reaction_count = promise.reactions.len();

        // PHANTOM_2M1_R1_PROMISE_ADOPTION_I
        // A reaction that returns another internal Promise must make its child
        // adopt that Promise's eventual state/value instead of fulfilling with
        // the Promise object itself. This is the core Promise Resolution
        // Procedure needed by fetch(...).then(r => r.text()).then(...).
        if let PromiseState::Fulfilled(Value::Object(returned_promise_id)) = &state
            && let Some(returned_promise) = self.promises.get(returned_promise_id)
        {
            if *returned_promise_id == promise_id {
                return self.settle_promise(
                    promise_id,
                    PromiseState::Rejected(Value::String(
                        "promise cannot resolve itself".to_owned(),
                    )),
                    offset,
                );
            }

            let returned_state = returned_promise.state.clone();
            let returned_reaction_count = returned_promise.reactions.len();

            if matches!(returned_state, PromiseState::Pending) {
                // PHANTOM_2M3_PROMISE_CYCLE_SAFETY_I
                // Pending adoption is an explicit dependency edge from the
                // returned Promise to the child being resolved. Before adding
                // that edge, walk existing reaction dependencies from the child
                // and reject if the returned Promise is already reachable.
                // This prevents A -> B -> A chains from remaining pending forever.
                let mut pending = vec![promise_id];
                let mut visited = Vec::new();
                let mut dependency_cycle = false;
                while let Some(current) = pending.pop() {
                    if current == *returned_promise_id {
                        dependency_cycle = true;
                        break;
                    }
                    if visited.contains(&current) {
                        continue;
                    }
                    if visited.len() >= MAX_PROMISES {
                        dependency_cycle = true;
                        break;
                    }
                    visited.push(current);
                    if let Some(current_promise) = self.promises.get(&current) {
                        for reaction in &current_promise.reactions {
                            if !visited.contains(&reaction.child)
                                && !pending.contains(&reaction.child)
                            {
                                pending.push(reaction.child);
                            }
                        }
                    }
                }
                if dependency_cycle {
                    return self.settle_promise(
                        promise_id,
                        PromiseState::Rejected(Value::String(
                            "promise resolution cycle".to_owned(),
                        )),
                        offset,
                    );
                }

                if returned_reaction_count >= MAX_PROMISE_REACTIONS_PER_PROMISE {
                    return Err(RuntimeError::new(
                        RuntimeErrorKind::PromiseReactionLimitExceeded,
                        offset,
                    ));
                }

                let Some(returned_promise) = self.promises.get_mut(returned_promise_id) else {
                    return Err(RuntimeError::new(
                        RuntimeErrorKind::InvalidMemberTarget,
                        offset,
                    ));
                };
                returned_promise.reactions.push(PromiseReaction {
                    on_fulfilled: None,
                    on_rejected: None,
                    child: promise_id,
                });
                return Ok(());
            }

            return self.settle_promise(promise_id, returned_state, offset);
        }

        // PHANTOM_2M4_THENABLE_ASSIMILATION_I
        // Ordinary objects with a callable `then` participate in the Promise
        // resolution procedure. The job is explicit so failures reject only
        // the target promise instead of aborting the whole checkpoint.
        if let PromiseState::Fulfilled(Value::Object(thenable_id)) = &state
            && !self.promises.contains_key(thenable_id)
        {
            let then_callback = self
                .objects
                .get(*thenable_id)
                .and_then(|object| object.get("then"))
                .filter(|value| is_callable(value))
                .cloned();
            if let Some(then_callback) = then_callback {
                if self.native_functions.len().saturating_add(2) > MAX_NATIVE_FUNCTIONS {
                    return Err(RuntimeError::new(
                        RuntimeErrorKind::NativeFunctionLimitExceeded,
                        offset,
                    ));
                }
                let resolve = self.allocate_native_function(
                    NativeFunctionKind::ThenableResolve(promise_id, *thenable_id),
                    offset,
                )?;
                let reject = self.allocate_native_function(
                    NativeFunctionKind::ThenableReject(promise_id),
                    offset,
                )?;
                self.enqueue_microtask(
                    Microtask::ThenableJob {
                        callback: then_callback,
                        arguments: vec![
                            Value::NativeFunction(resolve),
                            Value::NativeFunction(reject),
                        ],
                        promise_id,
                    },
                    offset,
                )?;
                return Ok(());
            }
        }

        if self.microtasks.len().saturating_add(reaction_count) > MAX_MICROTASKS {
            return Err(RuntimeError::new(
                RuntimeErrorKind::MicrotaskLimitExceeded,
                offset,
            ));
        }

        let reactions = {
            let Some(promise) = self.promises.get_mut(&promise_id) else {
                return Err(RuntimeError::new(
                    RuntimeErrorKind::InvalidMemberTarget,
                    offset,
                ));
            };
            promise.state = state.clone();
            std::mem::take(&mut promise.reactions)
        };

        for reaction in reactions {
            self.microtasks.push_back(Microtask::PromiseReaction {
                state: state.clone(),
                reaction,
            });
        }

        Ok(())
    }

    fn enqueue_microtask(
        &mut self,
        microtask: Microtask,
        offset: usize,
    ) -> Result<(), RuntimeError> {
        if self.microtasks.len() >= MAX_MICROTASKS {
            return Err(RuntimeError::new(
                RuntimeErrorKind::MicrotaskLimitExceeded,
                offset,
            ));
        }
        self.microtasks.push_back(microtask);
        Ok(())
    }

    fn perform_microtask_checkpoint(&mut self) -> Result<usize, RuntimeError> {
        let mut staged = self.clone();
        let processed = staged.perform_microtask_checkpoint_inner()?;
        *self = staged;
        Ok(processed)
    }

    fn perform_microtask_checkpoint_inner(&mut self) -> Result<usize, RuntimeError> {
        let mut processed = 0usize;

        while let Some(microtask) = self.microtasks.pop_front() {
            if processed >= MAX_MICROTASK_TASKS_PER_CHECKPOINT {
                return Err(RuntimeError::new(
                    RuntimeErrorKind::MicrotaskTaskLimitExceeded,
                    0,
                ));
            }

            self.steps = 0;
            match microtask {
                Microtask::Call {
                    callback,
                    arguments,
                } => {
                    self.call_value(callback, arguments, 0)?;
                }
                Microtask::ThenableJob {
                    callback,
                    arguments,
                    promise_id,
                } => {
                    // PHANTOM_2M4_R1_THENABLE_JOB_ISOLATION_I
                    // Errors in a thenable invocation reject only its target.
                    // A prior resolve/reject remains authoritative because
                    // settle_promise is idempotent after first settlement.
                    if let Err(error) = self.call_value(callback, arguments, 0) {
                        self.settle_promise(
                            promise_id,
                            PromiseState::Rejected(Value::String(error.to_string())),
                            0,
                        )?;
                    }
                }
                Microtask::PromiseReaction { state, reaction } => {
                    self.run_promise_reaction(state, reaction)?;
                }
            }
            processed = processed.saturating_add(1);
        }

        Ok(processed)
    }

    fn run_promise_reaction(
        &mut self,
        state: PromiseState,
        reaction: PromiseReaction,
    ) -> Result<(), RuntimeError> {
        match state {
            PromiseState::Pending => Ok(()),
            PromiseState::Fulfilled(value) => {
                if let Some(callback) = reaction.on_fulfilled {
                    let result = self.call_value(callback, vec![value], 0)?;
                    self.settle_promise(reaction.child, PromiseState::Fulfilled(result), 0)
                } else {
                    self.settle_promise(reaction.child, PromiseState::Fulfilled(value), 0)
                }
            }
            PromiseState::Rejected(reason) => {
                if let Some(callback) = reaction.on_rejected {
                    let result = self.call_value(callback, vec![reason], 0)?;
                    self.settle_promise(reaction.child, PromiseState::Fulfilled(result), 0)
                } else {
                    self.settle_promise(reaction.child, PromiseState::Rejected(reason), 0)
                }
            }
        }
    }

    fn schedule_timeout(
        &mut self,
        callback: usize,
        delay_ms: u64,
        offset: usize,
    ) -> Result<u32, RuntimeError> {
        if self.timers.len() >= MAX_TIMERS {
            return Err(RuntimeError::new(
                RuntimeErrorKind::TimerLimitExceeded,
                offset,
            ));
        }

        let id = self.next_timer_id;
        self.next_timer_id = self
            .next_timer_id
            .checked_add(1)
            .ok_or_else(|| RuntimeError::new(RuntimeErrorKind::TimerIdExhausted, offset))?;

        let sequence = self.next_timer_sequence;
        self.next_timer_sequence = self
            .next_timer_sequence
            .checked_add(1)
            .ok_or_else(|| RuntimeError::new(RuntimeErrorKind::TimerSequenceExhausted, offset))?;

        let due_at_ms = self
            .current_time_ms
            .checked_add(delay_ms)
            .ok_or_else(|| RuntimeError::new(RuntimeErrorKind::TimerClockOverflow, offset))?;

        self.timers.push(TimerRecord {
            id,
            due_at_ms,
            sequence,
            callback,
        });

        Ok(id)
    }

    fn advance_time_inner(&mut self, elapsed_ms: u64) -> Result<usize, RuntimeError> {
        self.current_time_ms = self
            .current_time_ms
            .checked_add(elapsed_ms)
            .ok_or_else(|| RuntimeError::new(RuntimeErrorKind::TimerClockOverflow, 0))?;

        let mut fired = 0usize;

        loop {
            let next = self
                .timers
                .iter()
                .enumerate()
                .filter(|(_, timer)| timer.due_at_ms <= self.current_time_ms)
                .min_by_key(|(_, timer)| (timer.due_at_ms, timer.sequence))
                .map(|(index, _)| index);

            let Some(index) = next else {
                break;
            };

            if fired >= MAX_TIMER_TASKS_PER_PUMP {
                return Err(RuntimeError::new(
                    RuntimeErrorKind::TimerTaskLimitExceeded,
                    0,
                ));
            }

            let timer = self.timers.remove(index);
            self.steps = 0;
            self.call_function(timer.callback, Vec::new(), Value::Undefined, 0)?;
            self.perform_microtask_checkpoint_inner()?;
            fired = fired.saturating_add(1);
        }

        Ok(fired)
    }

    // PHANTOM_2M11_DOM_EVENT_LISTENER_SEMANTICS_IV
    fn event_listener_options(
        &self,
        value: Option<&Value>,
        offset: usize,
    ) -> Result<(bool, bool, bool), RuntimeError> {
        match value {
            None | Some(Value::Undefined) => Ok((false, false, false)),
            Some(Value::Boolean(capture)) => Ok((*capture, false, false)),
            Some(Value::Object(object_id)) => {
                let Some(options) = self.objects.get(*object_id) else {
                    return Err(RuntimeError::new(
                        RuntimeErrorKind::UnsupportedEventListenerOptions,
                        offset,
                    ));
                };
                let capture = match options.get("capture") {
                    None | Some(Value::Undefined) => false,
                    Some(Value::Boolean(value)) => *value,
                    _ => {
                        return Err(RuntimeError::new(
                            RuntimeErrorKind::UnsupportedEventListenerOptions,
                            offset,
                        ));
                    }
                };
                let once = match options.get("once") {
                    None | Some(Value::Undefined) => false,
                    Some(Value::Boolean(value)) => *value,
                    _ => {
                        return Err(RuntimeError::new(
                            RuntimeErrorKind::UnsupportedEventListenerOptions,
                            offset,
                        ));
                    }
                };
                let passive = match options.get("passive") {
                    None | Some(Value::Undefined) => false,
                    Some(Value::Boolean(value)) => *value,
                    _ => {
                        return Err(RuntimeError::new(
                            RuntimeErrorKind::UnsupportedEventListenerOptions,
                            offset,
                        ));
                    }
                };
                Ok((capture, once, passive))
            }
            _ => Err(RuntimeError::new(
                RuntimeErrorKind::UnsupportedEventListenerOptions,
                offset,
            )),
        }
    }

    fn event_capture_option(
        &self,
        value: Option<&Value>,
        offset: usize,
    ) -> Result<bool, RuntimeError> {
        match value {
            None | Some(Value::Undefined) => Ok(false),
            Some(Value::Boolean(capture)) => Ok(*capture),
            Some(Value::Object(object_id)) => {
                let Some(options) = self.objects.get(*object_id) else {
                    return Err(RuntimeError::new(
                        RuntimeErrorKind::UnsupportedEventListenerOptions,
                        offset,
                    ));
                };
                match options.get("capture") {
                    None | Some(Value::Undefined) => Ok(false),
                    Some(Value::Boolean(value)) => Ok(*value),
                    _ => Err(RuntimeError::new(
                        RuntimeErrorKind::UnsupportedEventListenerOptions,
                        offset,
                    )),
                }
            }
            _ => Err(RuntimeError::new(
                RuntimeErrorKind::UnsupportedEventListenerOptions,
                offset,
            )),
        }
    }

    fn is_event_listener_callback(
        &mut self,
        value: &Value,
        offset: usize,
    ) -> Result<bool, RuntimeError> {
        if is_callable(value) {
            return Ok(true);
        }
        if matches!(value, Value::Object(_)) {
            return self
                .get_member(value, "handleEvent", offset)
                .map(|handle| is_callable(&handle));
        }
        Ok(false)
    }

    fn event_propagation_stopped(&self, event_id: usize) -> bool {
        self.event_dispatch_controls
            .iter()
            .rev()
            .find(|control| control.event_object == event_id)
            .is_some_and(|control| control.propagation_stopped)
    }

    fn event_immediate_stopped(&self, event_id: usize) -> bool {
        self.event_dispatch_controls
            .iter()
            .rev()
            .find(|control| control.event_object == event_id)
            .is_some_and(|control| control.immediate_stopped)
    }

    // PHANTOM_2M9_DOM_EVENT_PROPAGATION_II
    fn event_ancestor_path(
        &self,
        target_node_id: u64,
        offset: usize,
    ) -> Result<Vec<u64>, RuntimeError> {
        let mut path = Vec::new();
        let mut visited = BTreeSet::new();
        let mut current = self
            .host_element(target_node_id)
            .and_then(|element| element.parent_node_id);

        while let Some(node_id) = current {
            // PHANTOM_2M9_R1_ELEMENT_PROPAGATION_BOUNDARY_I
            // BrowserHostSnapshot intentionally contains elements only. The parent
            // of the root HTML element may therefore be a non-element Document
            // NodeId that is not materialized in `host_elements`. That is the
            // boundary of Event Propagation II: stop there instead of adding an
            // unmaterializable target to the element propagation path.
            let Some(element) = self.host_element(node_id) else {
                break;
            };
            if path.len() >= MAX_EVENT_PROPAGATION_PATH || !visited.insert(node_id) {
                return Err(RuntimeError::new(
                    RuntimeErrorKind::EventPropagationPathExceeded,
                    offset,
                ));
            }
            path.push(node_id);
            current = element.parent_node_id;
        }

        Ok(path)
    }

    fn dispatch_event_listeners_at(
        &mut self,
        target_object: usize,
        event_type: &str,
        capture: bool,
        event: &Value,
        original_target: &Value,
        event_phase: f64,
    ) -> Result<(), RuntimeError> {
        let offset = 0usize;
        let Value::Object(event_id) = event else {
            return Err(RuntimeError::new(
                RuntimeErrorKind::InvalidEventObject,
                offset,
            ));
        };
        let mut sequences = self
            .event_listeners
            .iter()
            .filter(|listener| {
                listener.target_object == target_object
                    && listener.event_type == event_type
                    && listener.capture == capture
            })
            .map(|listener| listener.sequence)
            .collect::<Vec<_>>();
        sequences.sort_unstable();

        let current_target = Value::Object(target_object);
        for sequence in sequences {
            let listener = self
                .event_listeners
                .iter()
                .find(|listener| {
                    listener.sequence == sequence
                        && listener.target_object == target_object
                        && listener.event_type == event_type
                        && listener.capture == capture
                })
                .cloned();
            let Some(listener) = listener else {
                continue;
            };

            // PHANTOM_2M10_DOM_EVENT_CONTROL_III
            // Remove a one-shot listener before invocation so nested/re-entrant
            // dispatch cannot observe it a second time.
            if listener.once {
                self.event_listeners
                    .retain(|registered| registered.sequence != sequence);
            }

            self.set_member(event, "target", original_target.clone(), offset)?;
            self.set_member(event, "currentTarget", current_target.clone(), offset)?;
            self.set_member(event, "eventPhase", Value::Number(event_phase), offset)?;

            // PHANTOM_2M11_DOM_EVENT_LISTENER_SEMANTICS_IV
            // EventListener callback objects invoke their current `handleEvent`
            // with the listener object as `this`; function listeners keep the
            // EventTarget as their receiver.
            let (callback, this_value) = if matches!(&listener.callback, Value::Object(_)) {
                (
                    self.get_member(&listener.callback, "handleEvent", offset)?,
                    listener.callback.clone(),
                )
            } else {
                (listener.callback.clone(), current_target.clone())
            };

            if let Some(control) = self
                .event_dispatch_controls
                .iter_mut()
                .rev()
                .find(|control| control.event_object == *event_id)
            {
                control.in_passive_listener = listener.passive;
            }
            let call_result =
                self.call_value_with_this(callback, vec![event.clone()], this_value, offset);
            if let Some(control) = self
                .event_dispatch_controls
                .iter_mut()
                .rev()
                .find(|control| control.event_object == *event_id)
            {
                control.in_passive_listener = false;
            }
            call_result?;
            if self.event_immediate_stopped(*event_id) {
                break;
            }
        }
        Ok(())
    }

    // PHANTOM_2M24_NATIVE_INTERACTION_JAVASCRIPT_EVENTS_I
    fn dispatch_event_object(
        &mut self,
        object_id: usize,
        node_id: u64,
        event_id: usize,
        offset: usize,
        is_trusted: bool,
    ) -> Result<bool, RuntimeError> {
        let event_type = self
            .objects
            .get(event_id)
            .and_then(|event| event.get("type"))
            .and_then(|value| match value {
                Value::String(value) => Some(value.clone()),
                _ => None,
            })
            .filter(|value| valid_event_type(value))
            .ok_or_else(|| RuntimeError::new(RuntimeErrorKind::InvalidEventObject, offset))?;
        let bubbles = self
            .objects
            .get(event_id)
            .and_then(|event| event.get("bubbles"))
            .and_then(|value| match value {
                Value::Boolean(value) => Some(*value),
                _ => None,
            })
            .unwrap_or(false);
        let cancelable = self
            .objects
            .get(event_id)
            .and_then(|event| event.get("cancelable"))
            .and_then(|value| match value {
                Value::Boolean(value) => Some(*value),
                _ => None,
            })
            .unwrap_or(false);

        if self.event_dispatch_depth >= MAX_EVENT_DISPATCH_DEPTH {
            return Err(RuntimeError::new(
                RuntimeErrorKind::EventDispatchDepthExceeded,
                offset,
            ));
        }
        if self.native_functions.len() >= MAX_NATIVE_FUNCTIONS {
            return Err(RuntimeError::new(
                RuntimeErrorKind::NativeFunctionLimitExceeded,
                offset,
            ));
        }

        let ancestor_nodes = self.event_ancestor_path(node_id, offset)?;
        let mut ancestor_objects = Vec::with_capacity(ancestor_nodes.len());
        for ancestor_node in &ancestor_nodes {
            let ancestor = self.materialize_host_element(*ancestor_node, offset)?;
            let Value::Object(ancestor_object) = ancestor else {
                return Err(RuntimeError::new(
                    RuntimeErrorKind::InvalidMemberTarget,
                    offset,
                ));
            };
            ancestor_objects.push(ancestor_object);
        }

        let prevent_default = self
            .allocate_native_function(NativeFunctionKind::EventPreventDefault(event_id), offset)?;
        let stop_propagation = self
            .allocate_native_function(NativeFunctionKind::EventStopPropagation(event_id), offset)?;
        let stop_immediate = self.allocate_native_function(
            NativeFunctionKind::EventStopImmediatePropagation(event_id),
            offset,
        )?;

        let target = Value::Object(object_id);
        let event = Value::Object(event_id);
        self.set_member(&event, "target", target.clone(), offset)?;
        self.set_member(&event, "currentTarget", Value::Null, offset)?;
        self.set_member(&event, "eventPhase", Value::Number(0.0), offset)?;
        self.set_member(&event, "bubbles", Value::Boolean(bubbles), offset)?;
        self.set_member(&event, "cancelable", Value::Boolean(cancelable), offset)?;
        self.set_member(&event, "defaultPrevented", Value::Boolean(false), offset)?;
        self.set_member(&event, "isTrusted", Value::Boolean(is_trusted), offset)?;
        self.set_member(
            &event,
            "preventDefault",
            Value::NativeFunction(prevent_default),
            offset,
        )?;
        self.set_member(
            &event,
            "stopPropagation",
            Value::NativeFunction(stop_propagation),
            offset,
        )?;
        self.set_member(
            &event,
            "stopImmediatePropagation",
            Value::NativeFunction(stop_immediate),
            offset,
        )?;

        self.event_dispatch_controls.push(EventDispatchControl {
            event_object: event_id,
            propagation_stopped: false,
            immediate_stopped: false,
            default_prevented: false,
            cancelable,
            in_passive_listener: false,
        });
        self.event_dispatch_depth = self.event_dispatch_depth.saturating_add(1);
        let dispatch_result = (|| -> Result<(), RuntimeError> {
            for ancestor_object in ancestor_objects.iter().rev().copied() {
                self.dispatch_event_listeners_at(
                    ancestor_object,
                    &event_type,
                    true,
                    &event,
                    &target,
                    1.0,
                )?;
                if self.event_propagation_stopped(event_id) {
                    break;
                }
            }

            if !self.event_propagation_stopped(event_id) {
                self.dispatch_event_listeners_at(
                    object_id,
                    &event_type,
                    true,
                    &event,
                    &target,
                    2.0,
                )?;
                if !self.event_immediate_stopped(event_id) {
                    self.dispatch_event_listeners_at(
                        object_id,
                        &event_type,
                        false,
                        &event,
                        &target,
                        2.0,
                    )?;
                }
            }

            if bubbles && !self.event_propagation_stopped(event_id) {
                for ancestor_object in ancestor_objects.iter().copied() {
                    self.dispatch_event_listeners_at(
                        ancestor_object,
                        &event_type,
                        false,
                        &event,
                        &target,
                        3.0,
                    )?;
                    if self.event_propagation_stopped(event_id) {
                        break;
                    }
                }
            }
            Ok(())
        })();
        self.event_dispatch_depth = self.event_dispatch_depth.saturating_sub(1);
        let control = self.event_dispatch_controls.pop();

        let clear_current = self.set_member(&event, "currentTarget", Value::Null, offset);
        let clear_phase = self.set_member(&event, "eventPhase", Value::Number(0.0), offset);
        let Some(control) = control else {
            return Err(RuntimeError::new(
                RuntimeErrorKind::InvalidEventObject,
                offset,
            ));
        };
        let persist_default = self.set_member(
            &event,
            "defaultPrevented",
            Value::Boolean(control.default_prevented),
            offset,
        );

        dispatch_result?;
        clear_current?;
        clear_phase?;
        persist_default?;
        Ok(!control.default_prevented)
    }

    fn call_host_function(
        &mut self,
        handle: usize,
        arguments: Vec<Value>,
        offset: usize,
    ) -> Result<Value, RuntimeError> {
        let object_id = handle / HOST_METHOD_COUNT;
        let method = handle % HOST_METHOD_COUNT;
        let Some(node_id) = self.host_object_nodes.get(&object_id).copied() else {
            return Err(RuntimeError::new(
                RuntimeErrorKind::InvalidMemberTarget,
                offset,
            ));
        };

        match method {
            HOST_METHOD_GET_ATTRIBUTE => {
                let name = arguments
                    .first()
                    .map(runtime_string)
                    .unwrap_or_default()
                    .to_ascii_lowercase();
                Ok(self
                    .host_element(node_id)
                    .and_then(|element| element.attributes.get(&name))
                    .cloned()
                    .map(Value::String)
                    .unwrap_or(Value::Null))
            }
            HOST_METHOD_SET_ATTRIBUTE => {
                let name = arguments
                    .first()
                    .map(runtime_string)
                    .unwrap_or_default()
                    .to_ascii_lowercase();
                let value = arguments.get(1).map(runtime_string).unwrap_or_default();
                self.update_host_attribute(node_id, &name, Some(value.clone()), offset)?;
                self.queue_dom_mutation(
                    BrowserDomMutation::SetAttribute {
                        node_id,
                        name,
                        value,
                    },
                    offset,
                )?;
                Ok(Value::Undefined)
            }
            HOST_METHOD_REMOVE_ATTRIBUTE => {
                let name = arguments
                    .first()
                    .map(runtime_string)
                    .unwrap_or_default()
                    .to_ascii_lowercase();
                self.update_host_attribute(node_id, &name, None, offset)?;
                self.queue_dom_mutation(
                    BrowserDomMutation::RemoveAttribute { node_id, name },
                    offset,
                )?;
                Ok(Value::Undefined)
            }
            HOST_METHOD_APPEND_CHILD => {
                let Some(Value::Object(child_object)) = arguments.first() else {
                    return Err(RuntimeError::new(
                        RuntimeErrorKind::InvalidMemberTarget,
                        offset,
                    ));
                };
                let Some(child_id) = self.host_object_nodes.get(child_object).copied() else {
                    return Err(RuntimeError::new(
                        RuntimeErrorKind::InvalidMemberTarget,
                        offset,
                    ));
                };
                if self.would_create_host_cycle(node_id, child_id) {
                    return Err(RuntimeError::new(
                        RuntimeErrorKind::InvalidDomHierarchy,
                        offset,
                    ));
                }
                let parent_connected = self
                    .host_element(node_id)
                    .is_some_and(|element| element.connected);
                // PHANTOM_2M20_DOM_TREE_NAVIGATION_MUTATION_COHERENCE_I
                // appendChild places the moved subtree after existing siblings in
                // the runtime snapshot before the canonical preorder rebuild.
                self.move_host_subtree_to_end(child_id);
                self.set_host_parent(child_id, Some(node_id), parent_connected, offset)?;
                self.queue_dom_mutation(
                    BrowserDomMutation::AppendChild {
                        parent_id: node_id,
                        child_id,
                    },
                    offset,
                )?;
                Ok(Value::Object(*child_object))
            }
            // PHANTOM_2M22_DOM_MUTATION_API_II
            HOST_METHOD_APPEND => {
                let nodes = self.host_element_argument_nodes(&arguments, offset)?;
                self.validate_host_reparenting(node_id, &nodes, offset)?;
                self.ensure_dom_mutation_capacity(nodes.len(), offset)?;
                for child_id in nodes {
                    self.append_host_node(node_id, child_id, offset)?;
                }
                Ok(Value::Undefined)
            }
            HOST_METHOD_PREPEND => {
                let nodes = self.host_element_argument_nodes(&arguments, offset)?;
                self.validate_host_reparenting(node_id, &nodes, offset)?;
                if let Some(reference_id) = self.direct_host_children(node_id).first().copied() {
                    self.insert_host_nodes_before(node_id, reference_id, &nodes, offset)?;
                } else {
                    self.ensure_dom_mutation_capacity(nodes.len(), offset)?;
                    for child_id in nodes {
                        self.append_host_node(node_id, child_id, offset)?;
                    }
                }
                Ok(Value::Undefined)
            }
            HOST_METHOD_BEFORE | HOST_METHOD_AFTER | HOST_METHOD_REPLACE_WITH => {
                let nodes = self.host_element_argument_nodes(&arguments, offset)?;
                let Some(parent_id) = self
                    .host_element(node_id)
                    .and_then(|element| element.parent_node_id)
                    .filter(|parent_id| self.host_element(*parent_id).is_some())
                else {
                    return Ok(Value::Undefined);
                };
                self.validate_host_reparenting(parent_id, &nodes, offset)?;

                let siblings = self.direct_host_children(parent_id);
                let reference_id =
                    if method == HOST_METHOD_BEFORE || method == HOST_METHOD_REPLACE_WITH {
                        Some(node_id)
                    } else {
                        siblings
                            .iter()
                            .position(|candidate| *candidate == node_id)
                            .and_then(|index| siblings.get(index.saturating_add(1)).copied())
                    };

                // Reserve conservatively before any runtime or mutation-queue change.
                self.ensure_dom_mutation_capacity(
                    nodes.len().saturating_add(siblings.len()).saturating_add(
                        if method == HOST_METHOD_REPLACE_WITH && !nodes.contains(&node_id) {
                            1
                        } else {
                            0
                        },
                    ),
                    offset,
                )?;

                if let Some(reference_id) = reference_id {
                    self.insert_host_nodes_before(parent_id, reference_id, &nodes, offset)?;
                } else {
                    for child_id in &nodes {
                        self.append_host_node(parent_id, *child_id, offset)?;
                    }
                }

                if method == HOST_METHOD_REPLACE_WITH && !nodes.contains(&node_id) {
                    self.detach_host_node(node_id, offset)?;
                }
                Ok(Value::Undefined)
            }
            HOST_METHOD_REPLACE_CHILDREN => {
                let nodes = self.host_element_argument_nodes(&arguments, offset)?;
                self.validate_host_reparenting(node_id, &nodes, offset)?;
                let existing = self.direct_host_children(node_id);
                self.ensure_dom_mutation_capacity(
                    existing.len().saturating_add(nodes.len()),
                    offset,
                )?;
                for child_id in existing {
                    self.detach_host_node(child_id, offset)?;
                }
                for child_id in nodes {
                    self.append_host_node(node_id, child_id, offset)?;
                }
                Ok(Value::Undefined)
            }
            HOST_METHOD_REMOVE_CHILD => {
                let Some(Value::Object(child_object)) = arguments.first() else {
                    return Err(RuntimeError::new(
                        RuntimeErrorKind::InvalidMemberTarget,
                        offset,
                    ));
                };
                if arguments.len() != 1 {
                    return Err(RuntimeError::new(
                        RuntimeErrorKind::ArgumentLimitExceeded,
                        offset,
                    ));
                }
                let Some(child_id) = self.host_object_nodes.get(child_object).copied() else {
                    return Err(RuntimeError::new(
                        RuntimeErrorKind::InvalidMemberTarget,
                        offset,
                    ));
                };
                if self
                    .host_element(child_id)
                    .and_then(|element| element.parent_node_id)
                    != Some(node_id)
                {
                    return Err(RuntimeError::new(
                        RuntimeErrorKind::InvalidDomHierarchy,
                        offset,
                    ));
                }
                self.ensure_dom_mutation_capacity(1, offset)?;
                self.detach_host_node(child_id, offset)?;
                Ok(Value::Object(*child_object))
            }
            HOST_METHOD_REMOVE => {
                if self
                    .host_element(node_id)
                    .and_then(|element| element.parent_node_id)
                    .is_some()
                {
                    self.ensure_dom_mutation_capacity(1, offset)?;
                    self.detach_host_node(node_id, offset)?;
                }
                Ok(Value::Undefined)
            }
            HOST_METHOD_HAS_ATTRIBUTE => {
                let name = arguments
                    .first()
                    .map(runtime_string)
                    .unwrap_or_default()
                    .to_ascii_lowercase();
                Ok(Value::Boolean(self.host_element(node_id).is_some_and(
                    |element| element.attributes.contains_key(&name),
                )))
            }
            // PHANTOM_2M14_DOM_DATASET_ATTRIBUTE_REFLECTION_I
            HOST_METHOD_TOGGLE_ATTRIBUTE => {
                if arguments.len() > 2 {
                    return Err(RuntimeError::new(
                        RuntimeErrorKind::ArgumentLimitExceeded,
                        offset,
                    ));
                }
                let name = arguments
                    .first()
                    .map(runtime_string)
                    .unwrap_or_default()
                    .to_ascii_lowercase();
                if name.is_empty()
                    || name.len() > MAX_HOST_TAG_BYTES
                    || name
                        .chars()
                        .any(|character| character.is_ascii_whitespace())
                {
                    return Err(RuntimeError::new(
                        RuntimeErrorKind::InvalidPropertyKey,
                        offset,
                    ));
                }
                let present = self
                    .host_element(node_id)
                    .is_some_and(|element| element.attributes.contains_key(&name));
                let should_have = arguments.get(1).map(is_truthy).unwrap_or(!present);
                if should_have != present {
                    if should_have {
                        self.update_host_attribute(node_id, &name, Some(String::new()), offset)?;
                        self.queue_dom_mutation(
                            BrowserDomMutation::SetAttribute {
                                node_id,
                                name,
                                value: String::new(),
                            },
                            offset,
                        )?;
                    } else {
                        self.update_host_attribute(node_id, &name, None, offset)?;
                        self.queue_dom_mutation(
                            BrowserDomMutation::RemoveAttribute { node_id, name },
                            offset,
                        )?;
                    }
                }
                Ok(Value::Boolean(should_have))
            }

            // PHANTOM_2M15_HTML_ATTRIBUTE_PROPERTY_REFLECTION_II
            HOST_METHOD_GET_ATTRIBUTE_NAMES => {
                let names = self
                    .host_element(node_id)
                    .map(|element| {
                        element
                            .attributes
                            .keys()
                            .cloned()
                            .map(Value::String)
                            .collect::<Vec<_>>()
                    })
                    .unwrap_or_default();
                self.allocate_array(names, offset)
            }
            HOST_METHOD_QUERY_SELECTOR => {
                let selector = arguments.first().map(runtime_string).unwrap_or_default();
                let node_ids =
                    self.query_host_selector_all(&selector, Some(node_id), false, offset)?;
                match node_ids.first().copied() {
                    Some(found) => self.materialize_host_element(found, offset),
                    None => Ok(Value::Null),
                }
            }
            HOST_METHOD_QUERY_SELECTOR_ALL => {
                let selector = arguments.first().map(runtime_string).unwrap_or_default();
                let node_ids =
                    self.query_host_selector_all(&selector, Some(node_id), false, offset)?;
                self.materialize_host_collection(&node_ids, offset)
            }
            HOST_METHOD_MATCHES => {
                let selector = arguments.first().map(runtime_string).unwrap_or_default();
                let Some(selector) = parse_host_selector(&selector) else {
                    return Err(RuntimeError::new(
                        RuntimeErrorKind::UnsupportedHostSelector,
                        offset,
                    ));
                };
                Ok(Value::Boolean(
                    self.host_selector_matches_node(node_id, &selector),
                ))
            }
            HOST_METHOD_CLOSEST => {
                let selector = arguments.first().map(runtime_string).unwrap_or_default();
                let Some(selector) = parse_host_selector(&selector) else {
                    return Err(RuntimeError::new(
                        RuntimeErrorKind::UnsupportedHostSelector,
                        offset,
                    ));
                };
                let mut current = Some(node_id);
                let mut visited = 0usize;
                while let Some(current_id) = current {
                    if visited >= MAX_SCOPE_DEPTH {
                        break;
                    }
                    visited = visited.saturating_add(1);
                    let Some(element) = self.host_element(current_id) else {
                        break;
                    };
                    let parent = element.parent_node_id;
                    if self.host_selector_matches_node(current_id, &selector) {
                        return self.materialize_host_element(current_id, offset);
                    }
                    current = parent;
                }
                Ok(Value::Null)
            }
            HOST_METHOD_CONTAINS => {
                let Some(Value::Object(other_object)) = arguments.first() else {
                    return Ok(Value::Boolean(false));
                };
                let Some(other_id) = self.host_object_nodes.get(other_object).copied() else {
                    return Ok(Value::Boolean(false));
                };
                Ok(Value::Boolean(
                    node_id == other_id || self.host_is_descendant_of(other_id, node_id),
                ))
            }
            // PHANTOM_2M12_DOM_TOKENLIST_CLASSLIST_I
            HOST_METHOD_CLASS_LIST_ADD => {
                let tokens = dom_token_arguments(&arguments, offset)?;
                let mut classes = self.host_class_tokens(node_id, offset)?;
                let mut changed = false;
                for token in tokens {
                    if !classes.iter().any(|class| class == &token) {
                        classes.push(token);
                        changed = true;
                    }
                }
                if changed {
                    self.commit_host_class_tokens(node_id, &classes, offset)?;
                }
                Ok(Value::Undefined)
            }
            HOST_METHOD_CLASS_LIST_REMOVE => {
                let tokens = dom_token_arguments(&arguments, offset)?;
                let mut classes = self.host_class_tokens(node_id, offset)?;
                let before = classes.len();
                classes.retain(|class| !tokens.iter().any(|token| token == class));
                if classes.len() != before {
                    self.commit_host_class_tokens(node_id, &classes, offset)?;
                }
                Ok(Value::Undefined)
            }
            HOST_METHOD_CLASS_LIST_CONTAINS => {
                let token = dom_token_argument(arguments.first(), offset)?;
                let classes = self.host_class_tokens(node_id, offset)?;
                Ok(Value::Boolean(classes.iter().any(|class| class == &token)))
            }
            HOST_METHOD_CLASS_LIST_TOGGLE => {
                let token = dom_token_argument(arguments.first(), offset)?;
                let force = arguments.get(1).map(is_truthy);
                let mut classes = self.host_class_tokens(node_id, offset)?;
                let present = classes.iter().any(|class| class == &token);
                let should_have = force.unwrap_or(!present);
                if should_have && !present {
                    classes.push(token);
                    self.commit_host_class_tokens(node_id, &classes, offset)?;
                } else if !should_have && present {
                    classes.retain(|class| class != &token);
                    self.commit_host_class_tokens(node_id, &classes, offset)?;
                }
                Ok(Value::Boolean(should_have))
            }

            // PHANTOM_2M13_CSS_STYLE_DECLARATION_I
            HOST_METHOD_STYLE_SET_PROPERTY => {
                if arguments.len() > 2 {
                    return Err(RuntimeError::new(
                        RuntimeErrorKind::ArgumentLimitExceeded,
                        offset,
                    ));
                }
                let property_name = style_property_name_argument(arguments.first(), offset)?;
                let value = arguments.get(1).map(runtime_string).unwrap_or_default();
                self.set_host_style_property(node_id, &property_name, value, offset)?;
                Ok(Value::Undefined)
            }
            HOST_METHOD_STYLE_GET_PROPERTY_VALUE => {
                let property_name = style_property_name_argument(arguments.first(), offset)?;
                self.host_style_property_value(node_id, &property_name, offset)
                    .map(Value::String)
            }
            HOST_METHOD_STYLE_REMOVE_PROPERTY => {
                let property_name = style_property_name_argument(arguments.first(), offset)?;
                self.remove_host_style_property(node_id, &property_name, offset)
                    .map(Value::String)
            }

            // PHANTOM_2M8_DOM_EVENT_TARGET_I
            HOST_METHOD_ADD_EVENT_LISTENER => {
                if arguments.len() > 3 {
                    return Err(RuntimeError::new(
                        RuntimeErrorKind::UnsupportedEventListenerOptions,
                        offset,
                    ));
                }
                let event_type = event_type_argument(arguments.first(), offset)?;
                let Some(callback) = arguments.get(1).cloned() else {
                    return Err(RuntimeError::new(
                        RuntimeErrorKind::InvalidEventCallback,
                        offset,
                    ));
                };
                if !self.is_event_listener_callback(&callback, offset)? {
                    return Err(RuntimeError::new(
                        RuntimeErrorKind::InvalidEventCallback,
                        offset,
                    ));
                }
                let (capture, once, passive) =
                    self.event_listener_options(arguments.get(2), offset)?;

                let duplicate = self.event_listeners.iter().any(|listener| {
                    listener.target_object == object_id
                        && listener.event_type == event_type
                        && listener.callback == callback
                        && listener.capture == capture
                });
                if duplicate {
                    return Ok(Value::Undefined);
                }
                if self.event_listeners.len() >= MAX_EVENT_LISTENERS {
                    return Err(RuntimeError::new(
                        RuntimeErrorKind::EventListenerLimitExceeded,
                        offset,
                    ));
                }
                let sequence = self.next_event_listener_sequence;
                self.next_event_listener_sequence = self
                    .next_event_listener_sequence
                    .checked_add(1)
                    .ok_or_else(|| {
                        RuntimeError::new(RuntimeErrorKind::EventListenerLimitExceeded, offset)
                    })?;
                self.event_listeners.push(EventListenerRecord {
                    target_object: object_id,
                    event_type,
                    callback,
                    capture,
                    once,
                    passive,
                    sequence,
                });
                Ok(Value::Undefined)
            }
            HOST_METHOD_REMOVE_EVENT_LISTENER => {
                if arguments.len() > 3 {
                    return Err(RuntimeError::new(
                        RuntimeErrorKind::UnsupportedEventListenerOptions,
                        offset,
                    ));
                }
                let event_type = event_type_argument(arguments.first(), offset)?;
                let Some(callback) = arguments.get(1).cloned() else {
                    return Ok(Value::Undefined);
                };
                if matches!(&callback, Value::Null | Value::Undefined) {
                    return Ok(Value::Undefined);
                }
                // PHANTOM_2M11_DOM_EVENT_LISTENER_SEMANTICS_IV
                // Removal identity is target/type/callback/capture only. `once`
                // and `passive` are intentionally ignored here.
                let capture = self.event_capture_option(arguments.get(2), offset)?;
                self.event_listeners.retain(|listener| {
                    !(listener.target_object == object_id
                        && listener.event_type == event_type
                        && listener.callback == callback
                        && listener.capture == capture)
                });
                Ok(Value::Undefined)
            }
            HOST_METHOD_DISPATCH_EVENT => {
                if arguments.len() != 1 {
                    return Err(RuntimeError::new(
                        RuntimeErrorKind::InvalidEventObject,
                        offset,
                    ));
                }
                let Some(Value::Object(event_id)) = arguments.first() else {
                    return Err(RuntimeError::new(
                        RuntimeErrorKind::InvalidEventObject,
                        offset,
                    ));
                };

                self.dispatch_event_object(object_id, node_id, *event_id, offset, false)
                    .map(Value::Boolean)
            }
            // PHANTOM_2M23_HTML_ELEMENT_INTERACTION_I
            HOST_METHOD_CLICK => {
                let disabled = self.host_element(node_id).is_some_and(|element| {
                    element.attributes.contains_key("disabled")
                        && matches!(
                            element.tag_name.as_str(),
                            "button"
                                | "input"
                                | "select"
                                | "textarea"
                                | "option"
                                | "optgroup"
                                | "fieldset"
                        )
                });
                if disabled {
                    return Ok(Value::Undefined);
                }

                let event = self.allocate_object(
                    BTreeMap::from([
                        ("type".to_owned(), Value::String("click".to_owned())),
                        ("bubbles".to_owned(), Value::Boolean(true)),
                        ("cancelable".to_owned(), Value::Boolean(true)),
                        ("isTrusted".to_owned(), Value::Boolean(false)),
                    ]),
                    offset,
                )?;
                let Value::Object(event_id) = event else {
                    return Err(RuntimeError::new(
                        RuntimeErrorKind::InvalidEventObject,
                        offset,
                    ));
                };
                let _ = self.call_host_function(
                    host_function_handle(object_id, HOST_METHOD_DISPATCH_EVENT),
                    vec![Value::Object(event_id)],
                    offset,
                )?;
                Ok(Value::Undefined)
            }
            _ => Err(RuntimeError::new(RuntimeErrorKind::CallNonFunction, offset)),
        }
    }

    fn query_host_selector(
        &self,
        selector: &str,
        offset: usize,
    ) -> Result<Option<u64>, RuntimeError> {
        Ok(self
            .query_host_selector_all(selector, None, true, offset)?
            .into_iter()
            .next())
    }

    fn query_host_selector_all(
        &self,
        selector: &str,
        scope: Option<u64>,
        require_connected: bool,
        offset: usize,
    ) -> Result<Vec<u64>, RuntimeError> {
        let Some(selector) = parse_host_selector(selector) else {
            return Err(RuntimeError::new(
                RuntimeErrorKind::UnsupportedHostSelector,
                offset,
            ));
        };

        Ok(self
            .host_elements
            .iter()
            .filter_map(|element| {
                let in_scope = match scope {
                    Some(scope_id) => self.host_is_descendant_of(element.node_id, scope_id),
                    None => true,
                };
                ((!require_connected || element.connected)
                    && in_scope
                    && self.host_selector_matches_node(element.node_id, &selector))
                .then_some(element.node_id)
            })
            .collect())
    }

    // PHANTOM_2M21_DOM_SELECTOR_API_COMPATIBILITY_II
    fn host_selector_matches_node(&self, node_id: u64, selector: &HostSelector) -> bool {
        selector
            .groups
            .iter()
            .any(|group| self.host_selector_group_matches(node_id, group))
    }

    fn host_selector_group_matches(&self, node_id: u64, group: &[HostSelectorStep]) -> bool {
        let Some(last_index) = group.len().checked_sub(1) else {
            return false;
        };
        self.host_selector_group_matches_at(node_id, group, last_index)
    }

    fn host_selector_group_matches_at(
        &self,
        node_id: u64,
        group: &[HostSelectorStep],
        index: usize,
    ) -> bool {
        let Some(step) = group.get(index) else {
            return false;
        };
        let Some(element) = self.host_element(node_id) else {
            return false;
        };
        if !host_selector_compound_matches(element, &step.compound) {
            return false;
        }
        if index == 0 {
            return true;
        }

        match step.relation_to_previous {
            Some(HostSelectorCombinator::Child) => {
                element.parent_node_id.is_some_and(|parent_id| {
                    self.host_selector_group_matches_at(parent_id, group, index - 1)
                })
            }
            Some(HostSelectorCombinator::Descendant) => {
                let mut current = element.parent_node_id;
                let mut visited = 0usize;
                while let Some(ancestor_id) = current {
                    if visited >= MAX_SCOPE_DEPTH {
                        return false;
                    }
                    visited = visited.saturating_add(1);
                    if self.host_selector_group_matches_at(ancestor_id, group, index - 1) {
                        return true;
                    }
                    current = self
                        .host_element(ancestor_id)
                        .and_then(|ancestor| ancestor.parent_node_id);
                }
                false
            }
            None => false,
        }
    }

    fn materialize_host_collection(
        &mut self,
        node_ids: &[u64],
        offset: usize,
    ) -> Result<Value, RuntimeError> {
        if node_ids.len() > MAX_ARRAY_LENGTH {
            return Err(RuntimeError::new(
                RuntimeErrorKind::ArrayLimitExceeded,
                offset,
            ));
        }
        let mut values = Vec::with_capacity(node_ids.len());
        for &node_id in node_ids {
            values.push(self.materialize_host_element(node_id, offset)?);
        }
        self.allocate_array(values, offset)
    }

    fn host_is_descendant_of(&self, node_id: u64, ancestor_id: u64) -> bool {
        if node_id == ancestor_id {
            return false;
        }
        let mut current = self
            .host_element(node_id)
            .and_then(|element| element.parent_node_id);
        let mut visited = 0usize;
        while let Some(current_id) = current {
            if current_id == ancestor_id {
                return true;
            }
            if visited >= MAX_SCOPE_DEPTH {
                return false;
            }
            visited = visited.saturating_add(1);
            current = self
                .host_element(current_id)
                .and_then(|element| element.parent_node_id);
        }
        false
    }

    // PHANTOM_2M12_DOM_TOKENLIST_CLASSLIST_I
    fn host_class_tokens(&self, node_id: u64, offset: usize) -> Result<Vec<String>, RuntimeError> {
        let Some(element) = self.host_element(node_id) else {
            return Err(RuntimeError::new(
                RuntimeErrorKind::InvalidMemberTarget,
                offset,
            ));
        };
        let mut seen = BTreeSet::new();
        let mut tokens = Vec::new();
        if let Some(value) = element.attributes.get("class") {
            for token in value.split_ascii_whitespace() {
                if seen.insert(token.to_owned()) {
                    tokens.push(token.to_owned());
                }
            }
        }
        Ok(tokens)
    }

    fn commit_host_class_tokens(
        &mut self,
        node_id: u64,
        classes: &[String],
        offset: usize,
    ) -> Result<(), RuntimeError> {
        let value = classes.join(" ");
        let Some(element) = self.host_element(node_id) else {
            return Err(RuntimeError::new(
                RuntimeErrorKind::InvalidMemberTarget,
                offset,
            ));
        };
        let retained_attribute_bytes = element
            .attributes
            .iter()
            .filter(|(name, _)| name.as_str() != "class")
            .fold(0usize, |total, (name, current)| {
                total
                    .saturating_add(name.len())
                    .saturating_add(current.len())
            })
            .saturating_add("class".len())
            .saturating_add(value.len());
        if retained_attribute_bytes > MAX_HOST_ATTRIBUTE_BYTES_PER_ELEMENT {
            return Err(RuntimeError::new(
                RuntimeErrorKind::HostSnapshotLimitExceeded,
                offset,
            ));
        }

        self.update_host_attribute(node_id, "class", Some(value.clone()), offset)?;
        self.queue_dom_mutation(
            BrowserDomMutation::SetAttribute {
                node_id,
                name: "class".to_owned(),
                value,
            },
            offset,
        )
    }

    // PHANTOM_2M13_CSS_STYLE_DECLARATION_I
    fn host_style_declarations(
        &self,
        node_id: u64,
        offset: usize,
    ) -> Result<Vec<(String, String)>, RuntimeError> {
        let Some(element) = self.host_element(node_id) else {
            return Err(RuntimeError::new(
                RuntimeErrorKind::InvalidMemberTarget,
                offset,
            ));
        };
        parse_inline_style_declarations(
            element
                .attributes
                .get("style")
                .map(String::as_str)
                .unwrap_or_default(),
            offset,
        )
    }

    fn host_style_css_text(&self, node_id: u64, offset: usize) -> Result<String, RuntimeError> {
        self.host_style_declarations(node_id, offset)
            .map(|declarations| serialize_inline_style_declarations(&declarations))
    }

    fn host_style_property_value(
        &self,
        node_id: u64,
        property_name: &str,
        offset: usize,
    ) -> Result<String, RuntimeError> {
        let canonical = canonical_style_property_name(property_name);
        self.host_style_declarations(node_id, offset)
            .map(|declarations| {
                declarations
                    .iter()
                    .find(|(name, _)| canonical_style_property_name(name) == canonical)
                    .map(|(_, value)| value.clone())
                    .unwrap_or_default()
            })
    }

    fn set_host_style_property(
        &mut self,
        node_id: u64,
        property_name: &str,
        value: String,
        offset: usize,
    ) -> Result<(), RuntimeError> {
        let property_name = normalize_style_property_name(property_name, offset)?;
        if value.len() > MAX_STYLE_VALUE_BYTES {
            return Err(RuntimeError::new(
                RuntimeErrorKind::HostSnapshotLimitExceeded,
                offset,
            ));
        }
        let value = value.trim().to_owned();
        if value.is_empty() {
            self.remove_host_style_property(node_id, &property_name, offset)?;
            return Ok(());
        }

        let canonical = canonical_style_property_name(&property_name);
        let mut declarations = self.host_style_declarations(node_id, offset)?;
        if let Some((name, current)) = declarations
            .iter_mut()
            .find(|(name, _)| canonical_style_property_name(name) == canonical)
        {
            *name = property_name;
            *current = value;
        } else {
            if declarations.len() >= MAX_STYLE_DECLARATIONS {
                return Err(RuntimeError::new(
                    RuntimeErrorKind::HostSnapshotLimitExceeded,
                    offset,
                ));
            }
            declarations.push((property_name, value));
        }
        self.commit_host_style_declarations(node_id, &declarations, offset)
    }

    fn remove_host_style_property(
        &mut self,
        node_id: u64,
        property_name: &str,
        offset: usize,
    ) -> Result<String, RuntimeError> {
        let property_name = normalize_style_property_name(property_name, offset)?;
        let canonical = canonical_style_property_name(&property_name);
        let mut declarations = self.host_style_declarations(node_id, offset)?;
        let removed = declarations
            .iter()
            .find(|(name, _)| canonical_style_property_name(name) == canonical)
            .map(|(_, value)| value.clone())
            .unwrap_or_default();
        declarations.retain(|(name, _)| canonical_style_property_name(name) != canonical);
        self.commit_host_style_declarations(node_id, &declarations, offset)?;
        Ok(removed)
    }

    fn set_host_style_css_text(
        &mut self,
        node_id: u64,
        css_text: String,
        offset: usize,
    ) -> Result<(), RuntimeError> {
        if css_text.len() > MAX_HOST_ATTRIBUTE_BYTES_PER_ELEMENT {
            return Err(RuntimeError::new(
                RuntimeErrorKind::HostSnapshotLimitExceeded,
                offset,
            ));
        }
        let declarations = parse_inline_style_declarations(&css_text, offset)?;
        self.commit_host_style_declarations(node_id, &declarations, offset)
    }

    fn commit_host_style_declarations(
        &mut self,
        node_id: u64,
        declarations: &[(String, String)],
        offset: usize,
    ) -> Result<(), RuntimeError> {
        let css_text = serialize_inline_style_declarations(declarations);
        let Some(element) = self.host_element(node_id) else {
            return Err(RuntimeError::new(
                RuntimeErrorKind::InvalidMemberTarget,
                offset,
            ));
        };
        let retained_attribute_bytes = element
            .attributes
            .iter()
            .filter(|(name, _)| name.as_str() != "style")
            .fold(0usize, |total, (name, current)| {
                total
                    .saturating_add(name.len())
                    .saturating_add(current.len())
            });
        let next_attribute_bytes = if css_text.is_empty() {
            retained_attribute_bytes
        } else {
            retained_attribute_bytes
                .saturating_add("style".len())
                .saturating_add(css_text.len())
        };
        if next_attribute_bytes > MAX_HOST_ATTRIBUTE_BYTES_PER_ELEMENT {
            return Err(RuntimeError::new(
                RuntimeErrorKind::HostSnapshotLimitExceeded,
                offset,
            ));
        }

        if css_text.is_empty() {
            self.update_host_attribute(node_id, "style", None, offset)?;
            self.queue_dom_mutation(
                BrowserDomMutation::RemoveAttribute {
                    node_id,
                    name: "style".to_owned(),
                },
                offset,
            )
        } else {
            self.update_host_attribute(node_id, "style", Some(css_text.clone()), offset)?;
            self.queue_dom_mutation(
                BrowserDomMutation::SetAttribute {
                    node_id,
                    name: "style".to_owned(),
                    value: css_text,
                },
                offset,
            )
        }
    }

    fn materialize_host_element(
        &mut self,
        node_id: u64,
        offset: usize,
    ) -> Result<Value, RuntimeError> {
        if let Some(object_id) = self.host_element_objects.get(&node_id).copied() {
            if let Some(element) = self.host_element(node_id).cloned() {
                self.refresh_host_element_object(object_id, &element)?;
            }
            return Ok(Value::Object(object_id));
        }

        let Some(element) = self.host_element(node_id).cloned() else {
            return Ok(Value::Null);
        };

        let object_id = object_id(self.allocate_object(BTreeMap::new(), offset)?, offset)?;
        self.host_element_objects.insert(node_id, object_id);
        self.host_object_nodes.insert(object_id, node_id);
        let class_list = self.ensure_host_class_list_object(object_id, offset)?;
        let style = self.ensure_host_style_object(object_id, offset)?;
        let dataset = self.ensure_host_dataset_object(object_id, offset)?;
        let properties =
            self.host_element_properties(&element, object_id, class_list, style, dataset);
        self.replace_host_object(object_id, properties)?;
        Ok(Value::Object(object_id))
    }

    fn refresh_host_element_object(
        &mut self,
        object_id: usize,
        element: &BrowserDomElementSnapshot,
    ) -> Result<(), RuntimeError> {
        let class_list = self.ensure_host_class_list_object(object_id, 0)?;
        let style = self.ensure_host_style_object(object_id, 0)?;
        let dataset = self.ensure_host_dataset_object(object_id, 0)?;
        let properties =
            self.host_element_properties(element, object_id, class_list, style, dataset);
        self.replace_host_object(object_id, properties)
    }

    fn ensure_host_class_list_object(
        &mut self,
        element_object_id: usize,
        offset: usize,
    ) -> Result<Value, RuntimeError> {
        if let Some(Value::Object(class_list_id)) = self
            .objects
            .get(element_object_id)
            .and_then(|object| object.get("classList"))
            .cloned()
        {
            let properties = self.host_class_list_properties(element_object_id);
            self.replace_host_object(class_list_id, properties)?;
            return Ok(Value::Object(class_list_id));
        }

        let class_list_id = object_id(self.allocate_object(BTreeMap::new(), offset)?, offset)?;
        let properties = self.host_class_list_properties(element_object_id);
        self.replace_host_object(class_list_id, properties)?;
        Ok(Value::Object(class_list_id))
    }

    fn host_class_list_properties(&self, element_object_id: usize) -> BTreeMap<String, Value> {
        BTreeMap::from([
            (
                "add".to_owned(),
                Value::HostFunction(host_function_handle(
                    element_object_id,
                    HOST_METHOD_CLASS_LIST_ADD,
                )),
            ),
            (
                "remove".to_owned(),
                Value::HostFunction(host_function_handle(
                    element_object_id,
                    HOST_METHOD_CLASS_LIST_REMOVE,
                )),
            ),
            (
                "contains".to_owned(),
                Value::HostFunction(host_function_handle(
                    element_object_id,
                    HOST_METHOD_CLASS_LIST_CONTAINS,
                )),
            ),
            (
                "toggle".to_owned(),
                Value::HostFunction(host_function_handle(
                    element_object_id,
                    HOST_METHOD_CLASS_LIST_TOGGLE,
                )),
            ),
        ])
    }

    // PHANTOM_2M13_CSS_STYLE_DECLARATION_I
    fn ensure_host_style_object(
        &mut self,
        element_object_id: usize,
        offset: usize,
    ) -> Result<Value, RuntimeError> {
        if let Some(Value::Object(style_id)) = self
            .objects
            .get(element_object_id)
            .and_then(|object| object.get("style"))
            .cloned()
        {
            self.host_style_object_elements
                .insert(style_id, element_object_id);
            let properties = self.host_style_properties(element_object_id);
            self.replace_host_object(style_id, properties)?;
            return Ok(Value::Object(style_id));
        }

        let style_id = object_id(self.allocate_object(BTreeMap::new(), offset)?, offset)?;
        self.host_style_object_elements
            .insert(style_id, element_object_id);
        let properties = self.host_style_properties(element_object_id);
        self.replace_host_object(style_id, properties)?;
        Ok(Value::Object(style_id))
    }

    fn host_style_properties(&self, element_object_id: usize) -> BTreeMap<String, Value> {
        BTreeMap::from([
            (
                "setProperty".to_owned(),
                Value::HostFunction(host_function_handle(
                    element_object_id,
                    HOST_METHOD_STYLE_SET_PROPERTY,
                )),
            ),
            (
                "getPropertyValue".to_owned(),
                Value::HostFunction(host_function_handle(
                    element_object_id,
                    HOST_METHOD_STYLE_GET_PROPERTY_VALUE,
                )),
            ),
            (
                "removeProperty".to_owned(),
                Value::HostFunction(host_function_handle(
                    element_object_id,
                    HOST_METHOD_STYLE_REMOVE_PROPERTY,
                )),
            ),
        ])
    }

    // PHANTOM_2M14_DOM_DATASET_ATTRIBUTE_REFLECTION_I
    fn ensure_host_dataset_object(
        &mut self,
        element_object_id: usize,
        offset: usize,
    ) -> Result<Value, RuntimeError> {
        if let Some(Value::Object(dataset_id)) = self
            .objects
            .get(element_object_id)
            .and_then(|object| object.get("dataset"))
            .cloned()
        {
            self.host_dataset_object_elements
                .insert(dataset_id, element_object_id);
            return Ok(Value::Object(dataset_id));
        }

        let dataset_id = object_id(self.allocate_object(BTreeMap::new(), offset)?, offset)?;
        self.host_dataset_object_elements
            .insert(dataset_id, element_object_id);
        Ok(Value::Object(dataset_id))
    }

    fn host_element_properties(
        &self,
        element: &BrowserDomElementSnapshot,
        object_id: usize,
        class_list: Value,
        style: Value,
        dataset: Value,
    ) -> BTreeMap<String, Value> {
        BTreeMap::from([
            ("nodeType".to_owned(), Value::Number(1.0)),
            (
                "tagName".to_owned(),
                Value::String(element.tag_name.to_ascii_uppercase()),
            ),
            (
                "id".to_owned(),
                Value::String(element.attributes.get("id").cloned().unwrap_or_default()),
            ),
            (
                "textContent".to_owned(),
                Value::String(element.text_content.clone()),
            ),
            (
                "className".to_owned(),
                Value::String(element.attributes.get("class").cloned().unwrap_or_default()),
            ),
            // PHANTOM_2M15_HTML_ATTRIBUTE_PROPERTY_REFLECTION_II
            (
                "title".to_owned(),
                Value::String(element.attributes.get("title").cloned().unwrap_or_default()),
            ),
            (
                "lang".to_owned(),
                Value::String(element.attributes.get("lang").cloned().unwrap_or_default()),
            ),
            (
                "dir".to_owned(),
                Value::String(element.attributes.get("dir").cloned().unwrap_or_default()),
            ),
            (
                "hidden".to_owned(),
                Value::Boolean(element.attributes.contains_key("hidden")),
            ),
            // PHANTOM_2M23_HTML_ELEMENT_INTERACTION_I
            (
                "name".to_owned(),
                Value::String(element.attributes.get("name").cloned().unwrap_or_default()),
            ),
            (
                "type".to_owned(),
                Value::String(element.attributes.get("type").cloned().unwrap_or_default()),
            ),
            (
                "value".to_owned(),
                Value::String(element.attributes.get("value").cloned().unwrap_or_default()),
            ),
            (
                "placeholder".to_owned(),
                Value::String(
                    element
                        .attributes
                        .get("placeholder")
                        .cloned()
                        .unwrap_or_default(),
                ),
            ),
            (
                "href".to_owned(),
                Value::String(element.attributes.get("href").cloned().unwrap_or_default()),
            ),
            (
                "target".to_owned(),
                Value::String(
                    element
                        .attributes
                        .get("target")
                        .cloned()
                        .unwrap_or_default(),
                ),
            ),
            (
                "rel".to_owned(),
                Value::String(element.attributes.get("rel").cloned().unwrap_or_default()),
            ),
            (
                "action".to_owned(),
                Value::String(
                    element
                        .attributes
                        .get("action")
                        .cloned()
                        .unwrap_or_default(),
                ),
            ),
            (
                "method".to_owned(),
                Value::String(
                    element
                        .attributes
                        .get("method")
                        .cloned()
                        .unwrap_or_default(),
                ),
            ),
            (
                "enctype".to_owned(),
                Value::String(
                    element
                        .attributes
                        .get("enctype")
                        .cloned()
                        .unwrap_or_default(),
                ),
            ),
            (
                "formAction".to_owned(),
                Value::String(
                    element
                        .attributes
                        .get("formaction")
                        .cloned()
                        .unwrap_or_default(),
                ),
            ),
            (
                "formMethod".to_owned(),
                Value::String(
                    element
                        .attributes
                        .get("formmethod")
                        .cloned()
                        .unwrap_or_default(),
                ),
            ),
            (
                "formEnctype".to_owned(),
                Value::String(
                    element
                        .attributes
                        .get("formenctype")
                        .cloned()
                        .unwrap_or_default(),
                ),
            ),
            (
                "src".to_owned(),
                Value::String(element.attributes.get("src").cloned().unwrap_or_default()),
            ),
            (
                "alt".to_owned(),
                Value::String(element.attributes.get("alt").cloned().unwrap_or_default()),
            ),
            (
                "htmlFor".to_owned(),
                Value::String(element.attributes.get("for").cloned().unwrap_or_default()),
            ),
            (
                "disabled".to_owned(),
                Value::Boolean(element.attributes.contains_key("disabled")),
            ),
            (
                "checked".to_owned(),
                Value::Boolean(element.attributes.contains_key("checked")),
            ),
            (
                "selected".to_owned(),
                Value::Boolean(element.attributes.contains_key("selected")),
            ),
            (
                "required".to_owned(),
                Value::Boolean(element.attributes.contains_key("required")),
            ),
            (
                "readOnly".to_owned(),
                Value::Boolean(element.attributes.contains_key("readonly")),
            ),
            (
                "multiple".to_owned(),
                Value::Boolean(element.attributes.contains_key("multiple")),
            ),
            (
                "autoFocus".to_owned(),
                Value::Boolean(element.attributes.contains_key("autofocus")),
            ),
            ("classList".to_owned(), class_list),
            ("style".to_owned(), style),
            ("dataset".to_owned(), dataset),
            ("isConnected".to_owned(), Value::Boolean(element.connected)),
            (
                "getAttribute".to_owned(),
                Value::HostFunction(host_function_handle(object_id, HOST_METHOD_GET_ATTRIBUTE)),
            ),
            (
                "setAttribute".to_owned(),
                Value::HostFunction(host_function_handle(object_id, HOST_METHOD_SET_ATTRIBUTE)),
            ),
            (
                "removeAttribute".to_owned(),
                Value::HostFunction(host_function_handle(
                    object_id,
                    HOST_METHOD_REMOVE_ATTRIBUTE,
                )),
            ),
            (
                "appendChild".to_owned(),
                Value::HostFunction(host_function_handle(object_id, HOST_METHOD_APPEND_CHILD)),
            ),
            // PHANTOM_2M22_DOM_MUTATION_API_II
            (
                "append".to_owned(),
                Value::HostFunction(host_function_handle(object_id, HOST_METHOD_APPEND)),
            ),
            (
                "prepend".to_owned(),
                Value::HostFunction(host_function_handle(object_id, HOST_METHOD_PREPEND)),
            ),
            (
                "before".to_owned(),
                Value::HostFunction(host_function_handle(object_id, HOST_METHOD_BEFORE)),
            ),
            (
                "after".to_owned(),
                Value::HostFunction(host_function_handle(object_id, HOST_METHOD_AFTER)),
            ),
            (
                "replaceWith".to_owned(),
                Value::HostFunction(host_function_handle(object_id, HOST_METHOD_REPLACE_WITH)),
            ),
            (
                "replaceChildren".to_owned(),
                Value::HostFunction(host_function_handle(
                    object_id,
                    HOST_METHOD_REPLACE_CHILDREN,
                )),
            ),
            (
                "removeChild".to_owned(),
                Value::HostFunction(host_function_handle(object_id, HOST_METHOD_REMOVE_CHILD)),
            ),
            (
                "remove".to_owned(),
                Value::HostFunction(host_function_handle(object_id, HOST_METHOD_REMOVE)),
            ),
            (
                "hasAttribute".to_owned(),
                Value::HostFunction(host_function_handle(object_id, HOST_METHOD_HAS_ATTRIBUTE)),
            ),
            (
                "getAttributeNames".to_owned(),
                Value::HostFunction(host_function_handle(
                    object_id,
                    HOST_METHOD_GET_ATTRIBUTE_NAMES,
                )),
            ),
            (
                "toggleAttribute".to_owned(),
                Value::HostFunction(host_function_handle(
                    object_id,
                    HOST_METHOD_TOGGLE_ATTRIBUTE,
                )),
            ),
            (
                "querySelector".to_owned(),
                Value::HostFunction(host_function_handle(object_id, HOST_METHOD_QUERY_SELECTOR)),
            ),
            (
                "querySelectorAll".to_owned(),
                Value::HostFunction(host_function_handle(
                    object_id,
                    HOST_METHOD_QUERY_SELECTOR_ALL,
                )),
            ),
            (
                "matches".to_owned(),
                Value::HostFunction(host_function_handle(object_id, HOST_METHOD_MATCHES)),
            ),
            (
                "closest".to_owned(),
                Value::HostFunction(host_function_handle(object_id, HOST_METHOD_CLOSEST)),
            ),
            (
                "contains".to_owned(),
                Value::HostFunction(host_function_handle(object_id, HOST_METHOD_CONTAINS)),
            ),
            (
                "addEventListener".to_owned(),
                Value::HostFunction(host_function_handle(
                    object_id,
                    HOST_METHOD_ADD_EVENT_LISTENER,
                )),
            ),
            (
                "removeEventListener".to_owned(),
                Value::HostFunction(host_function_handle(
                    object_id,
                    HOST_METHOD_REMOVE_EVENT_LISTENER,
                )),
            ),
            (
                "dispatchEvent".to_owned(),
                Value::HostFunction(host_function_handle(object_id, HOST_METHOD_DISPATCH_EVENT)),
            ),
            (
                "click".to_owned(),
                Value::HostFunction(host_function_handle(object_id, HOST_METHOD_CLICK)),
            ),
        ])
    }

    // PHANTOM_2M20_DOM_TREE_NAVIGATION_MUTATION_COHERENCE_I
    fn host_tree_navigation_member(
        &mut self,
        object_id: usize,
        key: &str,
        offset: usize,
    ) -> Result<Value, RuntimeError> {
        let Some(node_id) = self.host_object_nodes.get(&object_id).copied() else {
            return Err(RuntimeError::new(
                RuntimeErrorKind::InvalidMemberTarget,
                offset,
            ));
        };

        if self.host_element(node_id).is_none() {
            return Err(RuntimeError::new(
                RuntimeErrorKind::InvalidMemberTarget,
                offset,
            ));
        }

        match key {
            "isConnected" => Ok(Value::Boolean(
                self.host_element(node_id)
                    .is_some_and(|element| element.connected),
            )),
            "parentElement" => {
                let parent_id = self
                    .host_element(node_id)
                    .and_then(|element| element.parent_node_id)
                    .filter(|parent_id| self.host_element(*parent_id).is_some());
                match parent_id {
                    Some(parent_id) => self.materialize_host_element(parent_id, offset),
                    None => Ok(Value::Null),
                }
            }
            "children" => {
                let children = self
                    .host_elements
                    .iter()
                    .filter_map(|element| {
                        (element.parent_node_id == Some(node_id)).then_some(element.node_id)
                    })
                    .collect::<Vec<_>>();
                self.materialize_host_collection(&children, offset)
            }
            "childElementCount" => Ok(Value::Number(
                self.host_elements
                    .iter()
                    .filter(|element| element.parent_node_id == Some(node_id))
                    .count() as f64,
            )),
            "firstElementChild" | "lastElementChild" => {
                let child_id = if key == "firstElementChild" {
                    self.host_elements
                        .iter()
                        .find(|element| element.parent_node_id == Some(node_id))
                        .map(|element| element.node_id)
                } else {
                    self.host_elements
                        .iter()
                        .rev()
                        .find(|element| element.parent_node_id == Some(node_id))
                        .map(|element| element.node_id)
                };
                match child_id {
                    Some(child_id) => self.materialize_host_element(child_id, offset),
                    None => Ok(Value::Null),
                }
            }
            "previousElementSibling" | "nextElementSibling" => {
                let Some(parent_id) = self
                    .host_element(node_id)
                    .and_then(|element| element.parent_node_id)
                else {
                    return Ok(Value::Null);
                };
                let siblings = self
                    .host_elements
                    .iter()
                    .filter_map(|element| {
                        (element.parent_node_id == Some(parent_id)).then_some(element.node_id)
                    })
                    .collect::<Vec<_>>();
                let sibling_id = siblings
                    .iter()
                    .position(|candidate| *candidate == node_id)
                    .and_then(|index| {
                        if key == "previousElementSibling" {
                            index
                                .checked_sub(1)
                                .and_then(|previous| siblings.get(previous))
                                .copied()
                        } else {
                            siblings.get(index.saturating_add(1)).copied()
                        }
                    });
                match sibling_id {
                    Some(sibling_id) => self.materialize_host_element(sibling_id, offset),
                    None => Ok(Value::Null),
                }
            }
            _ => Ok(Value::Undefined),
        }
    }

    // PHANTOM_2M22_DOM_MUTATION_API_II
    fn host_element_argument_nodes(
        &self,
        arguments: &[Value],
        offset: usize,
    ) -> Result<Vec<u64>, RuntimeError> {
        if arguments.len() > MAX_DOM_MUTATION_ARGUMENTS {
            return Err(RuntimeError::new(
                RuntimeErrorKind::ArgumentLimitExceeded,
                offset,
            ));
        }
        arguments
            .iter()
            .map(|argument| {
                let Value::Object(object_id) = argument else {
                    return Err(RuntimeError::new(
                        RuntimeErrorKind::InvalidMemberTarget,
                        offset,
                    ));
                };
                let Some(node_id) = self.host_object_nodes.get(object_id).copied() else {
                    return Err(RuntimeError::new(
                        RuntimeErrorKind::InvalidMemberTarget,
                        offset,
                    ));
                };
                Ok(node_id)
            })
            .collect()
    }

    fn direct_host_children(&self, parent_id: u64) -> Vec<u64> {
        self.host_elements
            .iter()
            .filter_map(|element| {
                (element.parent_node_id == Some(parent_id)).then_some(element.node_id)
            })
            .collect()
    }

    fn validate_host_reparenting(
        &self,
        parent_id: u64,
        node_ids: &[u64],
        offset: usize,
    ) -> Result<(), RuntimeError> {
        if self.host_element(parent_id).is_none() {
            return Err(RuntimeError::new(
                RuntimeErrorKind::InvalidMemberTarget,
                offset,
            ));
        }
        for child_id in node_ids {
            if self.host_element(*child_id).is_none()
                || self.would_create_host_cycle(parent_id, *child_id)
            {
                return Err(RuntimeError::new(
                    RuntimeErrorKind::InvalidDomHierarchy,
                    offset,
                ));
            }
        }
        Ok(())
    }

    fn ensure_dom_mutation_capacity(
        &self,
        additional: usize,
        offset: usize,
    ) -> Result<(), RuntimeError> {
        if self.dom_mutations.len().saturating_add(additional) > MAX_DOM_MUTATIONS_PER_EXECUTION {
            return Err(RuntimeError::new(
                RuntimeErrorKind::DomMutationLimitExceeded,
                offset,
            ));
        }
        Ok(())
    }

    fn append_host_node(
        &mut self,
        parent_id: u64,
        child_id: u64,
        offset: usize,
    ) -> Result<(), RuntimeError> {
        let parent_connected = self
            .host_element(parent_id)
            .is_some_and(|element| element.connected);
        self.move_host_subtree_to_end(child_id);
        self.set_host_parent(child_id, Some(parent_id), parent_connected, offset)?;
        self.queue_dom_mutation(
            BrowserDomMutation::AppendChild {
                parent_id,
                child_id,
            },
            offset,
        )
    }

    fn detach_host_node(&mut self, node_id: u64, offset: usize) -> Result<(), RuntimeError> {
        self.set_host_parent(node_id, None, false, offset)?;
        self.queue_dom_mutation(BrowserDomMutation::DetachNode { node_id }, offset)
    }

    fn insert_host_nodes_before(
        &mut self,
        parent_id: u64,
        reference_id: u64,
        node_ids: &[u64],
        offset: usize,
    ) -> Result<(), RuntimeError> {
        let children = self.direct_host_children(parent_id);
        let Some(reference_index) = children
            .iter()
            .position(|candidate| *candidate == reference_id)
        else {
            return Err(RuntimeError::new(
                RuntimeErrorKind::InvalidDomHierarchy,
                offset,
            ));
        };
        let inserted = node_ids.iter().copied().collect::<BTreeSet<_>>();
        let tail = children[reference_index..]
            .iter()
            .copied()
            .filter(|child_id| !inserted.contains(child_id))
            .collect::<Vec<_>>();
        self.ensure_dom_mutation_capacity(node_ids.len().saturating_add(tail.len()), offset)?;
        for child_id in node_ids {
            self.append_host_node(parent_id, *child_id, offset)?;
        }
        for child_id in tail {
            self.append_host_node(parent_id, child_id, offset)?;
        }
        Ok(())
    }

    fn host_element(&self, node_id: u64) -> Option<&BrowserDomElementSnapshot> {
        self.host_elements
            .iter()
            .find(|element| element.node_id == node_id)
    }

    fn update_host_text_content(
        &mut self,
        node_id: u64,
        value: String,
        offset: usize,
    ) -> Result<(), RuntimeError> {
        if value.len() > MAX_HOST_TEXT_BYTES {
            return Err(RuntimeError::new(
                RuntimeErrorKind::HostSnapshotLimitExceeded,
                offset,
            ));
        }

        let Some(element) = self
            .host_elements
            .iter_mut()
            .find(|element| element.node_id == node_id)
        else {
            return Err(RuntimeError::new(
                RuntimeErrorKind::InvalidMemberTarget,
                offset,
            ));
        };
        element.text_content = value.clone();

        if let Some(object_id) = self.host_element_objects.get(&node_id).copied()
            && let Some(object) = self.objects.get_mut(object_id)
        {
            object.insert("textContent".to_owned(), Value::String(value));
        }

        Ok(())
    }

    fn update_host_attribute(
        &mut self,
        node_id: u64,
        name: &str,
        value: Option<String>,
        offset: usize,
    ) -> Result<(), RuntimeError> {
        if name.len() > MAX_HOST_TAG_BYTES {
            return Err(RuntimeError::new(
                RuntimeErrorKind::HostSnapshotLimitExceeded,
                offset,
            ));
        }

        let Some(element) = self
            .host_elements
            .iter_mut()
            .find(|element| element.node_id == node_id)
        else {
            return Err(RuntimeError::new(
                RuntimeErrorKind::InvalidMemberTarget,
                offset,
            ));
        };

        match value {
            Some(value) => {
                if !element.attributes.contains_key(name)
                    && element.attributes.len() >= MAX_HOST_ATTRIBUTES_PER_ELEMENT
                {
                    return Err(RuntimeError::new(
                        RuntimeErrorKind::HostSnapshotLimitExceeded,
                        offset,
                    ));
                }
                element.attributes.insert(name.to_owned(), value);
            }
            None => {
                element.attributes.remove(name);
            }
        }

        let attribute_bytes = element
            .attributes
            .iter()
            .fold(0usize, |total, (name, value)| {
                total.saturating_add(name.len()).saturating_add(value.len())
            });
        if attribute_bytes > MAX_HOST_ATTRIBUTE_BYTES_PER_ELEMENT {
            return Err(RuntimeError::new(
                RuntimeErrorKind::HostSnapshotLimitExceeded,
                offset,
            ));
        }

        if let Some(object_id) = self.host_element_objects.get(&node_id).copied()
            && let Some(object) = self.objects.get_mut(object_id)
        {
            if name == "id" {
                let id = element.attributes.get("id").cloned().unwrap_or_default();
                object.insert("id".to_owned(), Value::String(id));
            } else if name == "class" {
                let class_name = element.attributes.get("class").cloned().unwrap_or_default();
                object.insert("className".to_owned(), Value::String(class_name));
            } else if matches!(name, "title" | "lang" | "dir") {
                let reflected = element.attributes.get(name).cloned().unwrap_or_default();
                object.insert(name.to_owned(), Value::String(reflected));
            } else if name == "hidden" {
                object.insert(
                    "hidden".to_owned(),
                    Value::Boolean(element.attributes.contains_key("hidden")),
                );
            } else if let Some(property_name) = html_string_reflected_property(name) {
                let reflected = element.attributes.get(name).cloned().unwrap_or_default();
                object.insert(property_name.to_owned(), Value::String(reflected));
            } else if let Some(property_name) = html_boolean_reflected_property(name) {
                object.insert(
                    property_name.to_owned(),
                    Value::Boolean(element.attributes.contains_key(name)),
                );
            }
        }

        Ok(())
    }

    fn allocate_virtual_node_id(&mut self, offset: usize) -> Result<u64, RuntimeError> {
        if self.next_virtual_node_serial >= MAX_VIRTUAL_NODE_SERIAL {
            return Err(RuntimeError::new(
                RuntimeErrorKind::VirtualNodeIdExhausted,
                offset,
            ));
        }

        let node_id = u64::MAX.saturating_sub(self.next_virtual_node_serial);
        self.next_virtual_node_serial = self.next_virtual_node_serial.saturating_add(1);

        if node_id <= MAX_HOST_ELEMENTS as u64 {
            return Err(RuntimeError::new(
                RuntimeErrorKind::VirtualNodeIdExhausted,
                offset,
            ));
        }

        Ok(node_id)
    }

    fn would_create_host_cycle(&self, parent_id: u64, child_id: u64) -> bool {
        if parent_id == child_id {
            return true;
        }

        let mut current = Some(parent_id);
        let mut visited = 0usize;

        while let Some(node_id) = current {
            if node_id == child_id {
                return true;
            }
            if visited >= MAX_SCOPE_DEPTH {
                return true;
            }
            visited = visited.saturating_add(1);
            current = self
                .host_element(node_id)
                .and_then(|element| element.parent_node_id);
        }

        false
    }

    // PHANTOM_2M20_DOM_TREE_NAVIGATION_MUTATION_COHERENCE_I
    fn move_host_subtree_to_end(&mut self, root_id: u64) {
        let subtree_ids = self
            .host_elements
            .iter()
            .filter_map(|element| {
                (element.node_id == root_id || self.host_is_descendant_of(element.node_id, root_id))
                    .then_some(element.node_id)
            })
            .collect::<BTreeSet<_>>();
        if subtree_ids.is_empty() {
            return;
        }

        let mut retained = Vec::with_capacity(self.host_elements.len());
        let mut moved = Vec::new();
        for element in self.host_elements.drain(..) {
            if subtree_ids.contains(&element.node_id) {
                moved.push(element);
            } else {
                retained.push(element);
            }
        }
        retained.extend(moved);
        self.host_elements = retained;
    }

    fn set_host_parent(
        &mut self,
        child_id: u64,
        parent_id: Option<u64>,
        connected: bool,
        offset: usize,
    ) -> Result<(), RuntimeError> {
        if let Some(parent_id) = parent_id
            && self.host_element(parent_id).is_none()
        {
            return Err(RuntimeError::new(
                RuntimeErrorKind::InvalidMemberTarget,
                offset,
            ));
        }

        let Some(child) = self
            .host_elements
            .iter_mut()
            .find(|element| element.node_id == child_id)
        else {
            return Err(RuntimeError::new(
                RuntimeErrorKind::InvalidMemberTarget,
                offset,
            ));
        };
        child.parent_node_id = parent_id;

        self.set_host_subtree_connected(child_id, connected);
        self.reorder_host_elements();
        Ok(())
    }

    fn reorder_host_elements(&mut self) {
        if self.host_elements.len() < 2 {
            return;
        }

        let known_ids = self
            .host_elements
            .iter()
            .map(|element| element.node_id)
            .collect::<BTreeSet<_>>();
        let mut children = BTreeMap::<Option<u64>, Vec<u64>>::new();
        for element in &self.host_elements {
            let parent = element
                .parent_node_id
                .filter(|parent_id| known_ids.contains(parent_id));
            children.entry(parent).or_default().push(element.node_id);
        }

        let mut order = Vec::with_capacity(self.host_elements.len());
        let mut seen = BTreeSet::new();
        let roots = children.get(&None).cloned().unwrap_or_default();
        for root in roots {
            let mut stack = vec![root];
            while let Some(node_id) = stack.pop() {
                if !seen.insert(node_id) {
                    continue;
                }
                order.push(node_id);
                if let Some(node_children) = children.get(&Some(node_id)) {
                    stack.extend(node_children.iter().rev().copied());
                }
            }
        }

        for element in &self.host_elements {
            if seen.insert(element.node_id) {
                order.push(element.node_id);
            }
        }

        let mut elements = self
            .host_elements
            .drain(..)
            .map(|element| (element.node_id, element))
            .collect::<BTreeMap<_, _>>();
        self.host_elements = order
            .into_iter()
            .filter_map(|node_id| elements.remove(&node_id))
            .collect();
    }

    fn set_host_subtree_connected(&mut self, root_id: u64, connected: bool) {
        let mut stack = vec![root_id];
        let mut visited = 0usize;

        while let Some(node_id) = stack.pop() {
            if visited >= MAX_HOST_ELEMENTS {
                break;
            }
            visited = visited.saturating_add(1);

            if let Some(element) = self
                .host_elements
                .iter_mut()
                .find(|element| element.node_id == node_id)
            {
                element.connected = connected;
            }

            let children = self
                .host_elements
                .iter()
                .filter_map(|element| {
                    (element.parent_node_id == Some(node_id)).then_some(element.node_id)
                })
                .collect::<Vec<_>>();
            stack.extend(children);
        }
    }

    fn queue_dom_mutation(
        &mut self,
        mutation: BrowserDomMutation,
        offset: usize,
    ) -> Result<(), RuntimeError> {
        if self.dom_mutations.len() >= MAX_DOM_MUTATIONS_PER_EXECUTION {
            return Err(RuntimeError::new(
                RuntimeErrorKind::DomMutationLimitExceeded,
                offset,
            ));
        }
        self.dom_mutations.push(mutation);
        Ok(())
    }

    fn call_function(
        &mut self,
        function_id: usize,
        arguments: Vec<Value>,
        this_value: Value,
        offset: usize,
    ) -> Result<Value, RuntimeError> {
        if self.call_depth >= MAX_CALL_DEPTH {
            return Err(RuntimeError::new(
                RuntimeErrorKind::CallDepthExceeded,
                offset,
            ));
        }

        let Some(function) = self.functions.get(function_id).cloned() else {
            return Err(RuntimeError::new(RuntimeErrorKind::CallNonFunction, offset));
        };

        let captured_len = function.captured_scopes.len();
        let saved_scopes = std::mem::take(&mut self.scopes);
        let global_scope = saved_scopes
            .first()
            .cloned()
            .unwrap_or_else(Scope::function_scope);
        self.scopes = vec![global_scope];
        self.scopes.extend(function.captured_scopes.clone());
        self.call_depth = self.call_depth.saturating_add(1);

        let result = (|| -> Result<Completion, RuntimeError> {
            if let Some(name) = &function.name {
                self.push_block_scope(offset)?;
                self.declare_lexical(name.clone(), Value::Function(function_id), false, offset)?;
            }

            self.push_function_scope(offset)?;
            self.declare_lexical("this".to_owned(), this_value, false, offset)?;

            for (index, parameter) in function.parameters.iter().enumerate() {
                let value = arguments.get(index).cloned().unwrap_or(Value::Undefined);
                self.declare_lexical(parameter.clone(), value, true, offset)?;
            }

            self.execute_statements(&function.source, &function.body)
        })();

        let updated_global = self
            .scopes
            .first()
            .cloned()
            .unwrap_or_else(Scope::function_scope);
        let updated_captures = self
            .scopes
            .get(1..1usize.saturating_add(captured_len))
            .map(<[Scope]>::to_vec)
            .unwrap_or_default();

        self.scopes = saved_scopes;
        if let Some(global) = self.scopes.first_mut() {
            *global = updated_global;
        } else {
            self.scopes.push(updated_global);
        }
        if let Some(function) = self.functions.get_mut(function_id) {
            function.captured_scopes = updated_captures;
        }

        self.call_depth = self.call_depth.saturating_sub(1);

        match result? {
            Completion::Return(value) => Ok(value),
            Completion::Normal(_) => Ok(Value::Undefined),
        }
    }

    fn replace_host_object(
        &mut self,
        object_id: usize,
        properties: BTreeMap<String, Value>,
    ) -> Result<(), RuntimeError> {
        if properties.len() > MAX_OBJECT_PROPERTIES {
            return Err(RuntimeError::new(
                RuntimeErrorKind::PropertyLimitExceeded,
                0,
            ));
        }

        let Some(object) = self.objects.get_mut(object_id) else {
            return Err(RuntimeError::new(RuntimeErrorKind::InvalidMemberTarget, 0));
        };
        *object = properties;
        Ok(())
    }

    fn set_global_host_binding(&mut self, name: &str, value: Value) -> Result<(), RuntimeError> {
        let Some(global) = self.scopes.first_mut() else {
            return Err(RuntimeError::new(RuntimeErrorKind::ScopeLimitExceeded, 0));
        };

        if let Some(binding) = global.bindings.get_mut(name) {
            binding.value = value;
            binding.mutable = false;
            return Ok(());
        }

        if global.bindings.len() >= MAX_BINDINGS_PER_SCOPE {
            return Err(RuntimeError::new(RuntimeErrorKind::BindingLimitExceeded, 0));
        }

        global.bindings.insert(
            name.to_owned(),
            Binding {
                value,
                mutable: false,
            },
        );
        Ok(())
    }

    fn declare_lexical(
        &mut self,
        name: String,
        value: Value,
        mutable: bool,
        offset: usize,
    ) -> Result<(), RuntimeError> {
        let Some(scope) = self.scopes.last_mut() else {
            return Err(RuntimeError::new(
                RuntimeErrorKind::ScopeLimitExceeded,
                offset,
            ));
        };

        if scope.bindings.contains_key(&name) {
            return Err(RuntimeError::new(
                RuntimeErrorKind::DuplicateBinding,
                offset,
            ));
        }

        if scope.bindings.len() >= MAX_BINDINGS_PER_SCOPE {
            return Err(RuntimeError::new(
                RuntimeErrorKind::BindingLimitExceeded,
                offset,
            ));
        }

        scope.bindings.insert(name, Binding { value, mutable });
        Ok(())
    }

    fn declare_var(
        &mut self,
        name: String,
        value: Value,
        has_initializer: bool,
        offset: usize,
    ) -> Result<(), RuntimeError> {
        let Some(scope) = self
            .scopes
            .iter_mut()
            .rev()
            .find(|scope| scope.function_boundary)
        else {
            return Err(RuntimeError::new(
                RuntimeErrorKind::ScopeLimitExceeded,
                offset,
            ));
        };

        if let Some(existing) = scope.bindings.get_mut(&name) {
            if !existing.mutable {
                return Err(RuntimeError::new(
                    RuntimeErrorKind::DuplicateBinding,
                    offset,
                ));
            }
            if has_initializer {
                existing.value = value;
            }
            return Ok(());
        }

        if scope.bindings.len() >= MAX_BINDINGS_PER_SCOPE {
            return Err(RuntimeError::new(
                RuntimeErrorKind::BindingLimitExceeded,
                offset,
            ));
        }

        scope.bindings.insert(
            name,
            Binding {
                value,
                mutable: true,
            },
        );
        Ok(())
    }

    fn lookup(&self, name: &str, offset: usize) -> Result<Value, RuntimeError> {
        self.scopes
            .iter()
            .rev()
            .find_map(|scope| scope.bindings.get(name))
            .map(|binding| binding.value.clone())
            .ok_or_else(|| RuntimeError::new(RuntimeErrorKind::ReferenceError, offset))
    }

    fn assign(&mut self, name: &str, value: Value, offset: usize) -> Result<(), RuntimeError> {
        for scope in self.scopes.iter_mut().rev() {
            if let Some(binding) = scope.bindings.get_mut(name) {
                if !binding.mutable {
                    return Err(RuntimeError::new(RuntimeErrorKind::ConstAssignment, offset));
                }
                binding.value = value;
                return Ok(());
            }
        }

        Err(RuntimeError::new(RuntimeErrorKind::ReferenceError, offset))
    }

    fn push_block_scope(&mut self, offset: usize) -> Result<(), RuntimeError> {
        self.push_scope(Scope::block_scope(), offset)
    }

    fn push_function_scope(&mut self, offset: usize) -> Result<(), RuntimeError> {
        self.push_scope(Scope::function_scope(), offset)
    }

    fn push_scope(&mut self, scope: Scope, offset: usize) -> Result<(), RuntimeError> {
        if self.scopes.len() >= MAX_SCOPE_DEPTH {
            return Err(RuntimeError::new(
                RuntimeErrorKind::ScopeLimitExceeded,
                offset,
            ));
        }
        self.scopes.push(scope);
        Ok(())
    }

    fn pop_scope(&mut self) {
        if self.scopes.len() > 1 {
            self.scopes.pop();
        }
    }

    fn span_text<'a>(&self, source: &'a str, span: Span) -> Result<&'a str, RuntimeError> {
        source
            .get(span.start()..span.end())
            .ok_or_else(|| RuntimeError::new(RuntimeErrorKind::InvalidStringLiteral, span.start()))
    }

    fn tick(&mut self, offset: usize) -> Result<(), RuntimeError> {
        if self.steps >= MAX_RUNTIME_STEPS {
            return Err(RuntimeError::new(
                RuntimeErrorKind::StepLimitExceeded,
                offset,
            ));
        }
        self.steps = self.steps.saturating_add(1);
        Ok(())
    }
}

fn host_function_handle(object_id: usize, method: usize) -> usize {
    object_id
        .saturating_mul(HOST_METHOD_COUNT)
        .saturating_add(method)
}

fn parse_host_class_names(value: &str, offset: usize) -> Result<Vec<String>, RuntimeError> {
    let classes = value
        .split_ascii_whitespace()
        .map(str::to_owned)
        .collect::<Vec<_>>();
    if classes.iter().any(|class| !valid_host_name(class)) {
        return Err(RuntimeError::new(
            RuntimeErrorKind::UnsupportedHostSelector,
            offset,
        ));
    }
    Ok(classes)
}

fn host_has_all_classes(element: &BrowserDomElementSnapshot, classes: &[String]) -> bool {
    if classes.is_empty() {
        return false;
    }
    let Some(value) = element.attributes.get("class") else {
        return false;
    };
    classes.iter().all(|class| {
        value
            .split_ascii_whitespace()
            .any(|token| token == class.as_str())
    })
}

// PHANTOM_2M12_DOM_TOKENLIST_CLASSLIST_I
fn dom_token_argument(value: Option<&Value>, offset: usize) -> Result<String, RuntimeError> {
    let token = value.map(runtime_string).unwrap_or_default();
    if token.is_empty()
        || token.len() > MAX_DOM_TOKEN_BYTES
        || token.bytes().any(|byte| byte.is_ascii_whitespace())
    {
        return Err(RuntimeError::new(RuntimeErrorKind::InvalidDomToken, offset));
    }
    Ok(token)
}

fn dom_token_arguments(arguments: &[Value], offset: usize) -> Result<Vec<String>, RuntimeError> {
    if arguments.len() > MAX_DOM_TOKEN_ARGUMENTS {
        return Err(RuntimeError::new(
            RuntimeErrorKind::ArgumentLimitExceeded,
            offset,
        ));
    }
    arguments
        .iter()
        .map(|value| dom_token_argument(Some(value), offset))
        .collect()
}

// PHANTOM_2M14_DOM_DATASET_ATTRIBUTE_REFLECTION_I
fn dataset_attribute_name_from_member(key: &str) -> Option<String> {
    if key.is_empty() || key.len() > MAX_DOM_DATASET_KEY_BYTES {
        return None;
    }

    let mut attribute = String::with_capacity("data-".len().saturating_add(key.len() * 2));
    attribute.push_str("data-");
    for character in key.chars() {
        if character.is_ascii_uppercase() {
            attribute.push('-');
            attribute.push(character.to_ascii_lowercase());
        } else if character.is_ascii_alphanumeric() || character == '_' {
            attribute.push(character);
        } else {
            return None;
        }
        if attribute.len() > MAX_HOST_TAG_BYTES {
            return None;
        }
    }
    Some(attribute)
}

// PHANTOM_2M13_CSS_STYLE_DECLARATION_I
fn style_property_name_argument(
    value: Option<&Value>,
    offset: usize,
) -> Result<String, RuntimeError> {
    normalize_style_property_name(&value.map(runtime_string).unwrap_or_default(), offset)
}

fn normalize_style_property_name(raw: &str, offset: usize) -> Result<String, RuntimeError> {
    let name = raw.trim();
    if name.is_empty() || name.len() > MAX_STYLE_PROPERTY_NAME_BYTES {
        return Err(RuntimeError::new(
            RuntimeErrorKind::InvalidCssPropertyName,
            offset,
        ));
    }
    if !name
        .bytes()
        .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_'))
    {
        return Err(RuntimeError::new(
            RuntimeErrorKind::InvalidCssPropertyName,
            offset,
        ));
    }
    if name.starts_with("--") {
        if name.len() == 2 {
            return Err(RuntimeError::new(
                RuntimeErrorKind::InvalidCssPropertyName,
                offset,
            ));
        }
        Ok(name.to_owned())
    } else {
        Ok(name.to_ascii_lowercase())
    }
}

fn style_property_name_from_member(key: &str, offset: usize) -> Result<String, RuntimeError> {
    if key == "cssFloat" {
        return Ok("float".to_owned());
    }
    if key.starts_with("--") || key.contains('-') {
        return normalize_style_property_name(key, offset);
    }
    let mut property = String::with_capacity(key.len().saturating_add(8));
    for byte in key.bytes() {
        if byte.is_ascii_uppercase() {
            property.push('-');
            property.push(char::from(byte.to_ascii_lowercase()));
        } else if byte.is_ascii_alphanumeric() || byte == b'_' {
            property.push(char::from(byte));
        } else {
            return Err(RuntimeError::new(
                RuntimeErrorKind::InvalidCssPropertyName,
                offset,
            ));
        }
    }
    normalize_style_property_name(&property, offset)
}

fn canonical_style_property_name(name: &str) -> String {
    if name.starts_with("--") {
        name.to_owned()
    } else {
        name.to_ascii_lowercase()
    }
}

fn parse_inline_style_declarations(
    source: &str,
    offset: usize,
) -> Result<Vec<(String, String)>, RuntimeError> {
    let mut declarations: Vec<(String, String)> = Vec::new();
    let mut segment_start = 0usize;
    let mut quote = None::<u8>;
    let mut escaped = false;
    let mut paren_depth = 0usize;

    let mut push_segment = |segment: &str| -> Result<(), RuntimeError> {
        let Some(colon) = segment.find(':') else {
            return Ok(());
        };
        let raw_name = segment[..colon].trim();
        let value = segment[colon.saturating_add(1)..].trim();
        if raw_name.is_empty() || value.is_empty() {
            return Ok(());
        }
        if raw_name.len() > MAX_STYLE_PROPERTY_NAME_BYTES || value.len() > MAX_STYLE_VALUE_BYTES {
            return Err(RuntimeError::new(
                RuntimeErrorKind::HostSnapshotLimitExceeded,
                offset,
            ));
        }
        // Existing inline CSS may contain syntax beyond the direct-property subset.
        // Preserve any compact non-whitespace property token instead of dropping it.
        if raw_name.bytes().any(|byte| byte.is_ascii_whitespace()) {
            return Ok(());
        }
        let name = canonical_style_property_name(raw_name);
        if let Some((existing_name, existing_value)) = declarations
            .iter_mut()
            .find(|(existing_name, _)| canonical_style_property_name(existing_name) == name)
        {
            *existing_name = raw_name.to_owned();
            *existing_value = value.to_owned();
        } else {
            if declarations.len() >= MAX_STYLE_DECLARATIONS {
                return Err(RuntimeError::new(
                    RuntimeErrorKind::HostSnapshotLimitExceeded,
                    offset,
                ));
            }
            declarations.push((raw_name.to_owned(), value.to_owned()));
        }
        Ok(())
    };

    for (index, byte) in source.bytes().enumerate() {
        if escaped {
            escaped = false;
            continue;
        }
        if quote.is_some() && byte == b'\\' {
            escaped = true;
            continue;
        }
        if let Some(active_quote) = quote {
            if byte == active_quote {
                quote = None;
            }
            continue;
        }
        if matches!(byte, b'\'' | b'"') {
            quote = Some(byte);
            continue;
        }
        match byte {
            b'(' => paren_depth = paren_depth.saturating_add(1),
            b')' => paren_depth = paren_depth.saturating_sub(1),
            b';' if paren_depth == 0 => {
                push_segment(&source[segment_start..index])?;
                segment_start = index.saturating_add(1);
            }
            _ => {}
        }
    }
    push_segment(&source[segment_start..])?;
    Ok(declarations)
}

fn serialize_inline_style_declarations(declarations: &[(String, String)]) -> String {
    declarations
        .iter()
        .map(|(name, value)| format!("{name}: {value}"))
        .collect::<Vec<_>>()
        .join("; ")
}

fn is_callable(value: &Value) -> bool {
    matches!(
        value,
        Value::Function(_) | Value::NativeFunction(_) | Value::HostFunction(_)
    )
}

// PHANTOM_2M8_DOM_EVENT_TARGET_I
fn valid_event_type(value: &str) -> bool {
    !value.is_empty() && value.len() <= MAX_EVENT_TYPE_BYTES
}

fn event_type_argument(value: Option<&Value>, offset: usize) -> Result<String, RuntimeError> {
    match value {
        Some(Value::String(value)) if valid_event_type(value) => Ok(value.clone()),
        _ => Err(RuntimeError::new(
            RuntimeErrorKind::InvalidEventType,
            offset,
        )),
    }
}

fn timer_delay_ms(value: Option<&Value>) -> u64 {
    let Some(value) = value else {
        return 0;
    };

    let delay = to_number(value);
    if delay.is_nan() || delay <= 0.0 {
        return 0;
    }
    if delay.is_infinite() {
        return MAX_TIMER_DELAY_MS;
    }

    delay.trunc().min(MAX_TIMER_DELAY_MS as f64) as u64
}

fn timer_id_from_value(value: &Value) -> Option<u32> {
    let number = to_number(value);
    if !number.is_finite() || number <= 0.0 {
        return None;
    }

    let truncated = number.trunc();
    if truncated > f64::from(u32::MAX) {
        return None;
    }

    Some(truncated as u32)
}

fn valid_host_name(value: &str) -> bool {
    !value.is_empty()
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_'))
}

// PHANTOM_2M23_HTML_ELEMENT_INTERACTION_I
fn html_string_reflected_attribute(property_name: &str) -> Option<&'static str> {
    match property_name {
        "name" => Some("name"),
        "type" => Some("type"),
        "value" => Some("value"),
        "placeholder" => Some("placeholder"),
        "href" => Some("href"),
        "target" => Some("target"),
        "rel" => Some("rel"),
        "action" => Some("action"),
        "method" => Some("method"),
        "enctype" => Some("enctype"),
        "formAction" => Some("formaction"),
        "formMethod" => Some("formmethod"),
        "formEnctype" => Some("formenctype"),
        "src" => Some("src"),
        "alt" => Some("alt"),
        "htmlFor" => Some("for"),
        _ => None,
    }
}

fn html_boolean_reflected_attribute(property_name: &str) -> Option<&'static str> {
    match property_name {
        "disabled" => Some("disabled"),
        "checked" => Some("checked"),
        "selected" => Some("selected"),
        "required" => Some("required"),
        "readOnly" => Some("readonly"),
        "multiple" => Some("multiple"),
        "autoFocus" => Some("autofocus"),
        _ => None,
    }
}

fn html_string_reflected_property(attribute_name: &str) -> Option<&'static str> {
    match attribute_name {
        "name" => Some("name"),
        "type" => Some("type"),
        "value" => Some("value"),
        "placeholder" => Some("placeholder"),
        "href" => Some("href"),
        "target" => Some("target"),
        "rel" => Some("rel"),
        "action" => Some("action"),
        "method" => Some("method"),
        "enctype" => Some("enctype"),
        "formaction" => Some("formAction"),
        "formmethod" => Some("formMethod"),
        "formenctype" => Some("formEnctype"),
        "src" => Some("src"),
        "alt" => Some("alt"),
        "for" => Some("htmlFor"),
        _ => None,
    }
}

fn html_boolean_reflected_property(attribute_name: &str) -> Option<&'static str> {
    match attribute_name {
        "disabled" => Some("disabled"),
        "checked" => Some("checked"),
        "selected" => Some("selected"),
        "required" => Some("required"),
        "readonly" => Some("readOnly"),
        "multiple" => Some("multiple"),
        "autofocus" => Some("autoFocus"),
        _ => None,
    }
}

fn valid_host_tag_name(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= MAX_HOST_TAG_BYTES
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-')
}

fn parse_host_selector(selector: &str) -> Option<HostSelector> {
    let selector = selector.trim();
    if selector.is_empty() || selector.len() > MAX_HOST_SELECTOR_BYTES {
        return None;
    }

    let group_sources = split_host_selector_groups(selector)?;
    if group_sources.is_empty() || group_sources.len() > MAX_HOST_SELECTOR_GROUPS {
        return None;
    }

    let mut groups = Vec::with_capacity(group_sources.len());
    for group_source in group_sources {
        let group = parse_host_selector_group(group_source)?;
        if group.is_empty() || group.len() > MAX_HOST_SELECTOR_STEPS {
            return None;
        }
        groups.push(group);
    }

    Some(HostSelector { groups })
}

fn split_host_selector_groups(selector: &str) -> Option<Vec<&str>> {
    let bytes = selector.as_bytes();
    let mut groups = Vec::new();
    let mut start = 0usize;
    let mut bracket_depth = 0usize;
    let mut quote = None::<u8>;

    for (index, byte) in bytes.iter().copied().enumerate() {
        if let Some(active_quote) = quote {
            if byte == active_quote {
                quote = None;
            }
            continue;
        }
        match byte {
            b'\'' | b'"' if bracket_depth > 0 => quote = Some(byte),
            b'[' => bracket_depth = bracket_depth.checked_add(1)?,
            b']' => {
                if bracket_depth == 0 {
                    return None;
                }
                bracket_depth -= 1;
            }
            b',' if bracket_depth == 0 => {
                let group = selector.get(start..index)?.trim();
                if group.is_empty() {
                    return None;
                }
                groups.push(group);
                start = index.saturating_add(1);
            }
            _ => {}
        }
    }

    if bracket_depth != 0 || quote.is_some() {
        return None;
    }
    let tail = selector.get(start..)?.trim();
    if tail.is_empty() {
        return None;
    }
    groups.push(tail);
    Some(groups)
}

fn parse_host_selector_group(group: &str) -> Option<Vec<HostSelectorStep>> {
    let bytes = group.as_bytes();
    let mut steps = Vec::new();
    let mut start = 0usize;
    let mut index = 0usize;
    let mut bracket_depth = 0usize;
    let mut quote = None::<u8>;
    let mut pending_relation = None::<HostSelectorCombinator>;

    while index <= bytes.len() {
        let byte = bytes.get(index).copied();
        if let Some(active_quote) = quote {
            if byte == Some(active_quote) {
                quote = None;
            }
            index = index.saturating_add(1);
            continue;
        }

        match byte {
            Some(b'\'') | Some(b'"') if bracket_depth > 0 => {
                quote = byte;
                index = index.saturating_add(1);
            }
            Some(b'[') => {
                bracket_depth = bracket_depth.checked_add(1)?;
                index = index.saturating_add(1);
            }
            Some(b']') => {
                if bracket_depth == 0 {
                    return None;
                }
                bracket_depth -= 1;
                index = index.saturating_add(1);
            }
            Some(b'>') if bracket_depth == 0 => {
                let compound_source = group.get(start..index)?.trim();
                if !compound_source.is_empty() {
                    let compound = parse_host_selector_compound(compound_source)?;
                    steps.push(HostSelectorStep {
                        compound,
                        relation_to_previous: pending_relation.take(),
                    });
                } else if steps.is_empty() || pending_relation.is_some() {
                    return None;
                }
                pending_relation = Some(HostSelectorCombinator::Child);
                index = index.saturating_add(1);
                while bytes.get(index).is_some_and(u8::is_ascii_whitespace) {
                    index = index.saturating_add(1);
                }
                start = index;
            }
            Some(byte) if bracket_depth == 0 && byte.is_ascii_whitespace() => {
                let compound_source = group.get(start..index)?.trim();
                if !compound_source.is_empty() {
                    let compound = parse_host_selector_compound(compound_source)?;
                    steps.push(HostSelectorStep {
                        compound,
                        relation_to_previous: pending_relation.take(),
                    });
                }
                while bytes.get(index).is_some_and(u8::is_ascii_whitespace) {
                    index = index.saturating_add(1);
                }
                if bytes.get(index) == Some(&b'>') {
                    start = index;
                    continue;
                }
                if !steps.is_empty() {
                    pending_relation = Some(HostSelectorCombinator::Descendant);
                }
                start = index;
            }
            None => {
                if bracket_depth != 0 || quote.is_some() {
                    return None;
                }
                let compound_source = group.get(start..index)?.trim();
                if compound_source.is_empty() {
                    return None;
                }
                let compound = parse_host_selector_compound(compound_source)?;
                steps.push(HostSelectorStep {
                    compound,
                    relation_to_previous: pending_relation.take(),
                });
                break;
            }
            Some(_) => index = index.saturating_add(1),
        }

        if steps.len() > MAX_HOST_SELECTOR_STEPS {
            return None;
        }
    }

    if pending_relation.is_some() || steps.is_empty() {
        return None;
    }
    if steps.first()?.relation_to_previous.is_some()
        || steps
            .iter()
            .skip(1)
            .any(|step| step.relation_to_previous.is_none())
    {
        return None;
    }
    Some(steps)
}

fn parse_host_selector_compound(source: &str) -> Option<HostSelectorCompound> {
    let bytes = source.as_bytes();
    let mut index = 0usize;
    let mut tag = None;
    let mut id = None;
    let mut classes = Vec::new();
    let mut attributes = Vec::new();
    let mut tests = 0usize;

    if bytes.first() == Some(&b'*') {
        index = 1;
    } else if bytes.first().is_some_and(|byte| byte.is_ascii_alphabetic()) {
        let start = index;
        while bytes
            .get(index)
            .is_some_and(|byte| byte.is_ascii_alphanumeric() || matches!(*byte, b'-' | b'_'))
        {
            index = index.saturating_add(1);
        }
        let value = source.get(start..index)?;
        if !valid_host_tag_name(value) {
            return None;
        }
        tag = Some(value.to_ascii_lowercase());
        tests = tests.saturating_add(1);
    }

    while index < bytes.len() {
        match bytes[index] {
            b'#' => {
                if id.is_some() {
                    return None;
                }
                index = index.saturating_add(1);
                let start = index;
                while bytes.get(index).is_some_and(|byte| {
                    byte.is_ascii_alphanumeric() || matches!(*byte, b'-' | b'_')
                }) {
                    index = index.saturating_add(1);
                }
                let value = source.get(start..index)?;
                if !valid_host_name(value) {
                    return None;
                }
                id = Some(value.to_owned());
                tests = tests.saturating_add(1);
            }
            b'.' => {
                index = index.saturating_add(1);
                let start = index;
                while bytes.get(index).is_some_and(|byte| {
                    byte.is_ascii_alphanumeric() || matches!(*byte, b'-' | b'_')
                }) {
                    index = index.saturating_add(1);
                }
                let value = source.get(start..index)?;
                if !valid_host_name(value) {
                    return None;
                }
                classes.push(value.to_owned());
                tests = tests.saturating_add(1);
            }
            b'[' => {
                let start = index.saturating_add(1);
                index = start;
                let mut quote = None::<u8>;
                while index < bytes.len() {
                    let byte = bytes[index];
                    if let Some(active_quote) = quote {
                        if byte == active_quote {
                            quote = None;
                        }
                    } else if matches!(byte, b'\'' | b'"') {
                        quote = Some(byte);
                    } else if byte == b']' {
                        break;
                    }
                    index = index.saturating_add(1);
                }
                if index >= bytes.len() || quote.is_some() {
                    return None;
                }
                let body = source.get(start..index)?.trim();
                index = index.saturating_add(1);
                let (name, value) = if let Some((name, value)) = body.split_once('=') {
                    let name = name.trim().to_ascii_lowercase();
                    let raw_value = value.trim();
                    let value = if (raw_value.starts_with('"') && raw_value.ends_with('"'))
                        || (raw_value.starts_with('\'') && raw_value.ends_with('\''))
                    {
                        raw_value
                            .get(1..raw_value.len().saturating_sub(1))?
                            .to_owned()
                    } else {
                        raw_value.to_owned()
                    };
                    if !valid_host_name(&name) || value.is_empty() {
                        return None;
                    }
                    (name, Some(value))
                } else {
                    let name = body.to_ascii_lowercase();
                    if !valid_host_name(&name) {
                        return None;
                    }
                    (name, None)
                };
                attributes.push(HostSelectorAttribute { name, value });
                tests = tests.saturating_add(1);
            }
            _ => return None,
        }

        if tests > MAX_HOST_SELECTOR_TESTS_PER_COMPOUND {
            return None;
        }
    }

    if tag.is_none() && id.is_none() && classes.is_empty() && attributes.is_empty() {
        if source == "*" {
            return Some(HostSelectorCompound {
                tag: None,
                id: None,
                classes,
                attributes,
            });
        }
        return None;
    }

    Some(HostSelectorCompound {
        tag,
        id,
        classes,
        attributes,
    })
}

fn host_selector_compound_matches(
    element: &BrowserDomElementSnapshot,
    selector: &HostSelectorCompound,
) -> bool {
    if selector
        .tag
        .as_ref()
        .is_some_and(|tag| !element.tag_name.eq_ignore_ascii_case(tag))
    {
        return false;
    }
    if selector
        .id
        .as_ref()
        .is_some_and(|id| element.attributes.get("id").map(String::as_str) != Some(id.as_str()))
    {
        return false;
    }
    if selector.classes.iter().any(|class| {
        !element
            .attributes
            .get("class")
            .is_some_and(|value| value.split_ascii_whitespace().any(|token| token == class))
    }) {
        return false;
    }
    selector
        .attributes
        .iter()
        .all(|attribute| match &attribute.value {
            Some(expected) => {
                element.attributes.get(&attribute.name).map(String::as_str)
                    == Some(expected.as_str())
            }
            None => element.attributes.contains_key(&attribute.name),
        })
}

fn object_id(value: Value, offset: usize) -> Result<usize, RuntimeError> {
    match value {
        Value::Object(id) => Ok(id),
        _ => Err(RuntimeError::new(
            RuntimeErrorKind::InvalidMemberTarget,
            offset,
        )),
    }
}

fn decode_string_literal(text: &str, offset: usize) -> Result<Value, RuntimeError> {
    let mut chars = text.chars();
    let Some(quote) = chars.next() else {
        return Err(RuntimeError::new(
            RuntimeErrorKind::InvalidStringLiteral,
            offset,
        ));
    };

    if quote != '\'' && quote != '"' {
        return Err(RuntimeError::new(
            RuntimeErrorKind::InvalidStringLiteral,
            offset,
        ));
    }

    let Some(last) = text.chars().last() else {
        return Err(RuntimeError::new(
            RuntimeErrorKind::InvalidStringLiteral,
            offset,
        ));
    };
    if last != quote || text.len() < 2 {
        return Err(RuntimeError::new(
            RuntimeErrorKind::InvalidStringLiteral,
            offset,
        ));
    }

    let start = quote.len_utf8();
    let end = text.len().saturating_sub(last.len_utf8());
    let Some(inner) = text.get(start..end) else {
        return Err(RuntimeError::new(
            RuntimeErrorKind::InvalidStringLiteral,
            offset,
        ));
    };

    let mut output = String::with_capacity(inner.len());
    let mut inner_chars = inner.chars();

    while let Some(character) = inner_chars.next() {
        if character != '\\' {
            output.push(character);
        } else {
            let Some(escaped) = inner_chars.next() else {
                return Err(RuntimeError::new(
                    RuntimeErrorKind::InvalidStringLiteral,
                    offset,
                ));
            };
            match escaped {
                '\\' => output.push('\\'),
                '\'' => output.push('\''),
                '"' => output.push('"'),
                'n' => output.push('\n'),
                'r' => output.push('\r'),
                't' => output.push('\t'),
                'b' => output.push('\u{0008}'),
                'f' => output.push('\u{000c}'),
                'v' => output.push('\u{000b}'),
                '0' => output.push('\0'),
                'x' | 'u' => {
                    return Err(RuntimeError::new(
                        RuntimeErrorKind::InvalidStringLiteral,
                        offset,
                    ));
                }
                other => output.push(other),
            }
        }

        if output.len() > MAX_RUNTIME_STRING_BYTES {
            return Err(RuntimeError::new(
                RuntimeErrorKind::InvalidStringLiteral,
                offset,
            ));
        }
    }

    Ok(Value::String(output))
}

fn evaluate_unary(operator: UnaryOperator, value: Value) -> Value {
    match operator {
        UnaryOperator::Plus => Value::Number(to_number(&value)),
        UnaryOperator::Minus => Value::Number(-to_number(&value)),
        UnaryOperator::LogicalNot => Value::Boolean(!is_truthy(&value)),
    }
}

// PHANTOM_2M16_WEB_GLOBALS_CORE_BUILTINS_I
fn math_round_compat(value: f64) -> f64 {
    if !value.is_finite() || value == 0.0 {
        return value;
    }
    if (-0.5..0.0).contains(&value) || value == -0.5 {
        return -0.0;
    }
    (value + 0.5).floor()
}

fn math_min_compat(arguments: &[Value]) -> f64 {
    if arguments.is_empty() {
        return f64::INFINITY;
    }
    let mut result = f64::INFINITY;
    for value in arguments {
        let number = to_number(value);
        if number.is_nan() {
            return f64::NAN;
        }
        if number < result || (number == 0.0 && result == 0.0 && number.is_sign_negative()) {
            result = number;
        }
    }
    result
}

fn math_max_compat(arguments: &[Value]) -> f64 {
    if arguments.is_empty() {
        return f64::NEG_INFINITY;
    }
    let mut result = f64::NEG_INFINITY;
    for value in arguments {
        let number = to_number(value);
        if number.is_nan() {
            return f64::NAN;
        }
        if number > result || (number == 0.0 && result == 0.0 && number.is_sign_positive()) {
            result = number;
        }
    }
    result
}

fn parse_int_compat(arguments: &[Value]) -> f64 {
    let raw = arguments
        .first()
        .map(runtime_string)
        .unwrap_or_else(|| "undefined".to_owned());
    let mut text = raw.trim_start();
    let mut sign = 1.0;
    if let Some(rest) = text.strip_prefix('-') {
        sign = -1.0;
        text = rest;
    } else if let Some(rest) = text.strip_prefix('+') {
        text = rest;
    }

    let requested_radix = arguments.get(1).map(to_number).unwrap_or(0.0);
    let mut radix = if requested_radix == 0.0 || requested_radix.is_nan() {
        0u32
    } else if requested_radix.is_finite() {
        let truncated = requested_radix.trunc();
        if !(2.0..=36.0).contains(&truncated) {
            return f64::NAN;
        }
        truncated as u32
    } else {
        return f64::NAN;
    };

    let has_hex_prefix = text.starts_with("0x") || text.starts_with("0X");
    if radix == 0 {
        radix = if has_hex_prefix { 16 } else { 10 };
    }
    if radix == 16 && has_hex_prefix {
        text = &text[2..];
    }

    let mut value = 0.0f64;
    let mut digits = 0usize;
    for character in text.chars() {
        let Some(digit) = character.to_digit(radix) else {
            break;
        };
        value = value * f64::from(radix) + f64::from(digit);
        digits = digits.saturating_add(1);
        if digits >= MAX_RUNTIME_STRING_BYTES {
            break;
        }
    }
    if digits == 0 { f64::NAN } else { sign * value }
}

fn parse_float_compat(arguments: &[Value]) -> f64 {
    let raw = arguments
        .first()
        .map(runtime_string)
        .unwrap_or_else(|| "undefined".to_owned());
    let text = raw.trim_start();
    if text.starts_with("Infinity") || text.starts_with("+Infinity") {
        return f64::INFINITY;
    }
    if text.starts_with("-Infinity") {
        return f64::NEG_INFINITY;
    }

    let bytes = text.as_bytes();
    let mut index = 0usize;
    if bytes
        .first()
        .is_some_and(|byte| matches!(*byte, b'+' | b'-'))
    {
        index += 1;
    }
    let mut mantissa_digits = 0usize;
    while index < bytes.len() && bytes[index].is_ascii_digit() {
        index += 1;
        mantissa_digits += 1;
    }
    if index < bytes.len() && bytes[index] == b'.' {
        index += 1;
        while index < bytes.len() && bytes[index].is_ascii_digit() {
            index += 1;
            mantissa_digits += 1;
        }
    }
    if mantissa_digits == 0 {
        return f64::NAN;
    }

    let exponent_start = index;
    if index < bytes.len() && matches!(bytes[index], b'e' | b'E') {
        index += 1;
        if index < bytes.len() && matches!(bytes[index], b'+' | b'-') {
            index += 1;
        }
        let exponent_digits_start = index;
        while index < bytes.len() && bytes[index].is_ascii_digit() {
            index += 1;
        }
        if index == exponent_digits_start {
            index = exponent_start;
        }
    }

    text[..index].parse::<f64>().unwrap_or(f64::NAN)
}

// PHANTOM_2M17_WEB_CORE_LIBRARY_II
fn relative_index(value: Option<&Value>, length: usize, default_to_end: bool) -> usize {
    let Some(value) = value else {
        return if default_to_end { length } else { 0 };
    };
    if matches!(value, Value::Undefined) {
        return if default_to_end { length } else { 0 };
    }
    let number = to_number(value);
    if number.is_nan() {
        return 0;
    }
    if number == f64::INFINITY {
        return length;
    }
    if number == f64::NEG_INFINITY {
        return 0;
    }
    let integer = number.trunc();
    if integer < 0.0 {
        let distance = (-integer).min(length as f64) as usize;
        length.saturating_sub(distance)
    } else {
        (integer as usize).min(length)
    }
}

fn bounded_position(value: Option<&Value>, length: usize, default_to_end: bool) -> usize {
    let Some(value) = value else {
        return if default_to_end { length } else { 0 };
    };
    if matches!(value, Value::Undefined) {
        return if default_to_end { length } else { 0 };
    }
    let number = to_number(value);
    if number.is_nan() || number <= 0.0 {
        0
    } else if number.is_infinite() {
        length
    } else {
        (number.trunc() as usize).min(length)
    }
}

fn append_json_string(output: &mut String, value: &str) {
    output.push('"');
    for character in value.chars() {
        match character {
            '"' => output.push_str("\\\""),
            '\\' => output.push_str("\\\\"),
            '\u{08}' => output.push_str("\\b"),
            '\u{0C}' => output.push_str("\\f"),
            '\n' => output.push_str("\\n"),
            '\r' => output.push_str("\\r"),
            '\t' => output.push_str("\\t"),
            character if character <= '\u{1F}' => {
                use std::fmt::Write as _;
                let _ = write!(output, "\\u{:04x}", character as u32);
            }
            character => output.push(character),
        }
    }
    output.push('"');
}

struct JsonParser<'source, 'runtime> {
    source: &'source str,
    index: usize,
    runtime: &'runtime mut Runtime,
    offset: usize,
}

impl<'source, 'runtime> JsonParser<'source, 'runtime> {
    fn new(source: &'source str, runtime: &'runtime mut Runtime, offset: usize) -> Self {
        Self {
            source,
            index: 0,
            runtime,
            offset,
        }
    }

    fn parse(&mut self) -> Result<Value, RuntimeError> {
        self.skip_whitespace();
        let value = self.parse_value(0)?;
        self.skip_whitespace();
        if self.index != self.source.len() {
            return Err(RuntimeError::new(
                RuntimeErrorKind::InvalidJson,
                self.offset,
            ));
        }
        Ok(value)
    }

    fn parse_value(&mut self, depth: usize) -> Result<Value, RuntimeError> {
        if depth > MAX_JSON_DEPTH {
            return Err(RuntimeError::new(
                RuntimeErrorKind::InvalidJson,
                self.offset,
            ));
        }
        self.skip_whitespace();
        match self.peek_byte() {
            Some(b'n') if self.consume_literal("null") => Ok(Value::Null),
            Some(b't') if self.consume_literal("true") => Ok(Value::Boolean(true)),
            Some(b'f') if self.consume_literal("false") => Ok(Value::Boolean(false)),
            Some(b'"') => self.parse_string().map(Value::String),
            Some(b'[') => self.parse_array(depth + 1),
            Some(b'{') => self.parse_object(depth + 1),
            Some(b'-' | b'0'..=b'9') => self.parse_number().map(Value::Number),
            _ => Err(RuntimeError::new(
                RuntimeErrorKind::InvalidJson,
                self.offset,
            )),
        }
    }

    fn parse_array(&mut self, depth: usize) -> Result<Value, RuntimeError> {
        self.index += 1;
        self.skip_whitespace();
        let mut values = Vec::new();
        if self.peek_byte() == Some(b']') {
            self.index += 1;
            return self.runtime.allocate_array(values, self.offset);
        }
        loop {
            if values.len() >= MAX_ARRAY_LENGTH {
                return Err(RuntimeError::new(
                    RuntimeErrorKind::ArrayLimitExceeded,
                    self.offset,
                ));
            }
            values.push(self.parse_value(depth)?);
            self.skip_whitespace();
            match self.peek_byte() {
                Some(b',') => {
                    self.index += 1;
                    self.skip_whitespace();
                }
                Some(b']') => {
                    self.index += 1;
                    break;
                }
                _ => {
                    return Err(RuntimeError::new(
                        RuntimeErrorKind::InvalidJson,
                        self.offset,
                    ));
                }
            }
        }
        self.runtime.allocate_array(values, self.offset)
    }

    fn parse_object(&mut self, depth: usize) -> Result<Value, RuntimeError> {
        self.index += 1;
        self.skip_whitespace();
        let mut values = BTreeMap::new();
        if self.peek_byte() == Some(b'}') {
            self.index += 1;
            return self.runtime.allocate_object(values, self.offset);
        }
        loop {
            if values.len() >= MAX_OBJECT_PROPERTIES {
                return Err(RuntimeError::new(
                    RuntimeErrorKind::PropertyLimitExceeded,
                    self.offset,
                ));
            }
            if self.peek_byte() != Some(b'"') {
                return Err(RuntimeError::new(
                    RuntimeErrorKind::InvalidJson,
                    self.offset,
                ));
            }
            let key = self.parse_string()?;
            self.skip_whitespace();
            if self.peek_byte() != Some(b':') {
                return Err(RuntimeError::new(
                    RuntimeErrorKind::InvalidJson,
                    self.offset,
                ));
            }
            self.index += 1;
            let value = self.parse_value(depth)?;
            values.insert(key, value);
            self.skip_whitespace();
            match self.peek_byte() {
                Some(b',') => {
                    self.index += 1;
                    self.skip_whitespace();
                }
                Some(b'}') => {
                    self.index += 1;
                    break;
                }
                _ => {
                    return Err(RuntimeError::new(
                        RuntimeErrorKind::InvalidJson,
                        self.offset,
                    ));
                }
            }
        }
        self.runtime.allocate_object(values, self.offset)
    }

    fn parse_string(&mut self) -> Result<String, RuntimeError> {
        self.index += 1;
        let mut output = String::new();
        while self.index < self.source.len() {
            let rest = &self.source[self.index..];
            let Some(character) = rest.chars().next() else {
                break;
            };
            self.index += character.len_utf8();
            match character {
                '"' => return Ok(output),
                '\\' => {
                    let Some(escape) = self.peek_byte() else {
                        return Err(RuntimeError::new(
                            RuntimeErrorKind::InvalidJson,
                            self.offset,
                        ));
                    };
                    self.index += 1;
                    match escape {
                        b'"' => output.push('"'),
                        b'\\' => output.push('\\'),
                        b'/' => output.push('/'),
                        b'b' => output.push('\u{08}'),
                        b'f' => output.push('\u{0C}'),
                        b'n' => output.push('\n'),
                        b'r' => output.push('\r'),
                        b't' => output.push('\t'),
                        b'u' => {
                            let high = self.parse_hex_quad()?;
                            if (0xD800..=0xDBFF).contains(&high) {
                                if self
                                    .source
                                    .as_bytes()
                                    .get(self.index..self.index.saturating_add(2))
                                    != Some(&b"\\u"[..])
                                {
                                    return Err(RuntimeError::new(
                                        RuntimeErrorKind::InvalidJson,
                                        self.offset,
                                    ));
                                }
                                self.index += 2;
                                let low = self.parse_hex_quad()?;
                                if !(0xDC00..=0xDFFF).contains(&low) {
                                    return Err(RuntimeError::new(
                                        RuntimeErrorKind::InvalidJson,
                                        self.offset,
                                    ));
                                }
                                let scalar = 0x10000
                                    + (((high - 0xD800) as u32) << 10)
                                    + (low - 0xDC00) as u32;
                                let Some(decoded) = char::from_u32(scalar) else {
                                    return Err(RuntimeError::new(
                                        RuntimeErrorKind::InvalidJson,
                                        self.offset,
                                    ));
                                };
                                output.push(decoded);
                            } else if (0xDC00..=0xDFFF).contains(&high) {
                                return Err(RuntimeError::new(
                                    RuntimeErrorKind::InvalidJson,
                                    self.offset,
                                ));
                            } else {
                                let Some(decoded) = char::from_u32(high as u32) else {
                                    return Err(RuntimeError::new(
                                        RuntimeErrorKind::InvalidJson,
                                        self.offset,
                                    ));
                                };
                                output.push(decoded);
                            }
                        }
                        _ => {
                            return Err(RuntimeError::new(
                                RuntimeErrorKind::InvalidJson,
                                self.offset,
                            ));
                        }
                    }
                }
                character if character <= '\u{1F}' => {
                    return Err(RuntimeError::new(
                        RuntimeErrorKind::InvalidJson,
                        self.offset,
                    ));
                }
                character => output.push(character),
            }
            if output.len() > MAX_RUNTIME_STRING_BYTES {
                return Err(RuntimeError::new(
                    RuntimeErrorKind::StringLimitExceeded,
                    self.offset,
                ));
            }
        }
        Err(RuntimeError::new(
            RuntimeErrorKind::InvalidJson,
            self.offset,
        ))
    }

    fn parse_hex_quad(&mut self) -> Result<u16, RuntimeError> {
        let bytes = self.source.as_bytes();
        if self.index.saturating_add(4) > bytes.len() {
            return Err(RuntimeError::new(
                RuntimeErrorKind::InvalidJson,
                self.offset,
            ));
        }
        let mut value = 0u16;
        for _ in 0..4 {
            let digit = match bytes[self.index] {
                b'0'..=b'9' => u16::from(bytes[self.index] - b'0'),
                b'a'..=b'f' => u16::from(bytes[self.index] - b'a' + 10),
                b'A'..=b'F' => u16::from(bytes[self.index] - b'A' + 10),
                _ => {
                    return Err(RuntimeError::new(
                        RuntimeErrorKind::InvalidJson,
                        self.offset,
                    ));
                }
            };
            value = value * 16 + digit;
            self.index += 1;
        }
        Ok(value)
    }

    fn parse_number(&mut self) -> Result<f64, RuntimeError> {
        let start = self.index;
        if self.peek_byte() == Some(b'-') {
            self.index += 1;
        }
        match self.peek_byte() {
            Some(b'0') => self.index += 1,
            Some(b'1'..=b'9') => {
                self.index += 1;
                while self.peek_byte().is_some_and(|byte| byte.is_ascii_digit()) {
                    self.index += 1;
                }
            }
            _ => {
                return Err(RuntimeError::new(
                    RuntimeErrorKind::InvalidJson,
                    self.offset,
                ));
            }
        }
        if self.peek_byte() == Some(b'.') {
            self.index += 1;
            let fraction_start = self.index;
            while self.peek_byte().is_some_and(|byte| byte.is_ascii_digit()) {
                self.index += 1;
            }
            if self.index == fraction_start {
                return Err(RuntimeError::new(
                    RuntimeErrorKind::InvalidJson,
                    self.offset,
                ));
            }
        }
        if self
            .peek_byte()
            .is_some_and(|byte| matches!(byte, b'e' | b'E'))
        {
            self.index += 1;
            if self
                .peek_byte()
                .is_some_and(|byte| matches!(byte, b'+' | b'-'))
            {
                self.index += 1;
            }
            let exponent_start = self.index;
            while self.peek_byte().is_some_and(|byte| byte.is_ascii_digit()) {
                self.index += 1;
            }
            if self.index == exponent_start {
                return Err(RuntimeError::new(
                    RuntimeErrorKind::InvalidJson,
                    self.offset,
                ));
            }
        }
        self.source[start..self.index]
            .parse::<f64>()
            .ok()
            .filter(|value| value.is_finite())
            .ok_or_else(|| RuntimeError::new(RuntimeErrorKind::InvalidJson, self.offset))
    }

    fn consume_literal(&mut self, literal: &str) -> bool {
        if self.source[self.index..].starts_with(literal) {
            self.index += literal.len();
            true
        } else {
            false
        }
    }

    fn skip_whitespace(&mut self) {
        while self
            .peek_byte()
            .is_some_and(|byte| matches!(byte, b' ' | b'\n' | b'\r' | b'\t'))
        {
            self.index += 1;
        }
    }

    fn peek_byte(&self) -> Option<u8> {
        self.source.as_bytes().get(self.index).copied()
    }
}

fn is_truthy(value: &Value) -> bool {
    match value {
        Value::Undefined | Value::Null => false,
        Value::Boolean(value) => *value,
        Value::Number(value) => *value != 0.0 && !value.is_nan(),
        Value::String(value) => !value.is_empty(),
        Value::Array(_)
        | Value::Object(_)
        | Value::Function(_)
        | Value::NativeFunction(_)
        | Value::HostFunction(_) => true,
    }
}

fn to_number(value: &Value) -> f64 {
    match value {
        Value::Undefined => f64::NAN,
        Value::Null => 0.0,
        Value::Boolean(false) => 0.0,
        Value::Boolean(true) => 1.0,
        Value::Number(value) => *value,
        Value::String(value) => {
            let trimmed = value.trim();
            if trimmed.is_empty() {
                0.0
            } else {
                trimmed.parse::<f64>().unwrap_or(f64::NAN)
            }
        }
        Value::Array(_)
        | Value::Object(_)
        | Value::Function(_)
        | Value::NativeFunction(_)
        | Value::HostFunction(_) => f64::NAN,
    }
}

fn runtime_string(value: &Value) -> String {
    match value {
        Value::Undefined => "undefined".to_owned(),
        Value::Null => "null".to_owned(),
        Value::Boolean(value) => value.to_string(),
        Value::Number(value) if value.is_nan() => "NaN".to_owned(),
        Value::Number(value) if value.is_infinite() && value.is_sign_positive() => {
            "Infinity".to_owned()
        }
        Value::Number(value) if value.is_infinite() => "-Infinity".to_owned(),
        Value::Number(value) if *value == 0.0 => "0".to_owned(),
        Value::Number(value) if value.fract() == 0.0 => format!("{value:.0}"),
        Value::Number(value) => value.to_string(),
        Value::String(value) => value.clone(),
        Value::Array(_) => "[object Array]".to_owned(),
        Value::Object(_) => "[object Object]".to_owned(),
        Value::Function(_) | Value::NativeFunction(_) | Value::HostFunction(_) => {
            "function".to_owned()
        }
    }
}

// PHANTOM_2N16_FETCH_REQUEST_INIT_HOST_BOUNDARY_I
fn canonical_fetch_method(value: &str, offset: usize) -> Result<String, RuntimeError> {
    if value.is_empty() || value.len() > 16 || !value.bytes().all(|byte| byte.is_ascii_alphabetic())
    {
        return Err(RuntimeError::new(
            RuntimeErrorKind::UnsupportedFetchOptions,
            offset,
        ));
    }
    let method = value.to_ascii_uppercase();
    if matches!(method.as_str(), "CONNECT" | "TRACE" | "TRACK") {
        return Err(RuntimeError::new(
            RuntimeErrorKind::UnsupportedFetchOptions,
            offset,
        ));
    }
    if !matches!(method.as_str(), "GET" | "POST" | "PUT" | "PATCH") {
        return Err(RuntimeError::new(
            RuntimeErrorKind::UnsupportedFetchOptions,
            offset,
        ));
    }
    Ok(method)
}

fn property_key(value: &Value, offset: usize) -> Result<String, RuntimeError> {
    match value {
        Value::Undefined
        | Value::Null
        | Value::Boolean(_)
        | Value::Number(_)
        | Value::String(_) => Ok(runtime_string(value)),
        Value::Array(_)
        | Value::Object(_)
        | Value::Function(_)
        | Value::NativeFunction(_)
        | Value::HostFunction(_) => Err(RuntimeError::new(
            RuntimeErrorKind::InvalidPropertyKey,
            offset,
        )),
    }
}

fn canonical_array_index(key: &str) -> Option<usize> {
    if key.is_empty() || !key.bytes().all(|byte| byte.is_ascii_digit()) {
        return None;
    }

    let index = key.parse::<usize>().ok()?;
    if index.to_string() == key {
        Some(index)
    } else {
        None
    }
}

fn strict_equal(left: &Value, right: &Value) -> bool {
    match (left, right) {
        (Value::Undefined, Value::Undefined) | (Value::Null, Value::Null) => true,
        (Value::Boolean(left), Value::Boolean(right)) => left == right,
        (Value::Number(left), Value::Number(right)) => left == right,
        (Value::String(left), Value::String(right)) => left == right,
        (Value::Array(left), Value::Array(right))
        | (Value::Object(left), Value::Object(right))
        | (Value::Function(left), Value::Function(right))
        | (Value::NativeFunction(left), Value::NativeFunction(right))
        | (Value::HostFunction(left), Value::HostFunction(right)) => left == right,
        _ => false,
    }
}

fn abstract_equal(left: &Value, right: &Value) -> bool {
    if strict_equal(left, right) {
        return true;
    }

    match (left, right) {
        (Value::Undefined, Value::Null) | (Value::Null, Value::Undefined) => true,
        (Value::Number(left), Value::String(right)) => {
            *left == to_number(&Value::String(right.clone()))
        }
        (Value::String(left), Value::Number(right)) => {
            to_number(&Value::String(left.clone())) == *right
        }
        (Value::Boolean(_), _) => abstract_equal(&Value::Number(to_number(left)), right),
        (_, Value::Boolean(_)) => abstract_equal(left, &Value::Number(to_number(right))),
        _ => false,
    }
}

fn relational_compare(left: &Value, right: &Value, compare: impl FnOnce(f64, f64) -> bool) -> bool {
    if let (Value::String(left), Value::String(right)) = (left, right) {
        return match left.as_bytes().cmp(right.as_bytes()) {
            std::cmp::Ordering::Less => compare(-1.0, 0.0),
            std::cmp::Ordering::Equal => compare(0.0, 0.0),
            std::cmp::Ordering::Greater => compare(1.0, 0.0),
        };
    }

    let left = to_number(left);
    let right = to_number(right);
    if left.is_nan() || right.is_nan() {
        false
    } else {
        compare(left, right)
    }
}

#[cfg(test)]
mod request_init_2n16_tests {
    use super::{BrowserHostSnapshot, FetchCredentialsMode, Runtime, RuntimeErrorKind};

    fn runtime() -> Runtime {
        let mut runtime = Runtime::new();
        let snapshot =
            BrowserHostSnapshot::new("https://app.example.test/page", "2N16", 1280.0, 720.0, 1.0)
                .unwrap_or_else(|error| panic!("snapshot failed: {error}"));
        runtime
            .install_browser_host(&snapshot)
            .unwrap_or_else(|error| panic!("host install failed: {error}"));
        runtime
    }

    #[test]
    fn request_init_emits_bounded_structured_host_request() {
        let mut runtime = runtime();
        runtime
            .execute(
                r#"fetch('/api/items', { method: 'post', headers: { 'Content-Type': 'application/json', 'X-Phantom': 'yes' }, body: '{\"ok\":true}', credentials: 'include' });"#,
            )
            .unwrap_or_else(|error| panic!("fetch init failed: {error}"));

        let requests = runtime.drain_fetch_host_requests();
        assert_eq!(requests.len(), 1);
        let request = &requests[0];
        assert_eq!(request.url(), "/api/items");
        assert_eq!(request.method(), "POST");
        assert_eq!(request.credentials(), FetchCredentialsMode::Include);
        assert_eq!(request.body(), Some(r#"{"ok":true}"#));
        assert!(
            request
                .headers()
                .iter()
                .any(|(name, value)| name == "content-type" && value == "application/json")
        );
        assert!(
            request
                .headers()
                .iter()
                .any(|(name, value)| name == "x-phantom" && value == "yes")
        );
    }

    #[test]
    fn request_init_defaults_and_invalid_combinations_fail_closed() {
        let mut default_runtime = runtime();
        default_runtime
            .execute("fetch('/plain');")
            .unwrap_or_else(|error| panic!("default fetch failed: {error}"));
        let request = default_runtime.drain_fetch_host_requests().remove(0);
        assert_eq!(request.method(), "GET");
        assert_eq!(request.credentials(), FetchCredentialsMode::SameOrigin);
        assert!(request.headers().is_empty());
        assert_eq!(request.body(), None);

        let mut body_runtime = runtime();
        let error = body_runtime
            .execute("fetch('/bad', { method: 'GET', body: 'x' });")
            .err()
            .unwrap_or_else(|| panic!("GET body must fail"));
        assert_eq!(error.kind(), RuntimeErrorKind::UnsupportedFetchOptions);

        let mut method_runtime = runtime();
        let error = method_runtime
            .execute("fetch('/bad', { method: 'DELETE' });")
            .err()
            .unwrap_or_else(|| panic!("unsupported method must fail"));
        assert_eq!(error.kind(), RuntimeErrorKind::UnsupportedFetchOptions);
    }
}

// PHANTOM_2N18_RESPONSE_HEADERS_EXPOSURE_I_TESTS
#[cfg(test)]
mod response_headers_2n18_tests {
    use super::*;

    #[test]
    fn allowlist_is_case_insensitive_and_rejects_forbidden_headers() {
        let accepted =
            validate_response_headers_2n18(&[("Content-Type".to_owned(), "text/plain".to_owned())])
                .unwrap();
        assert_eq!(
            accepted.get("content-type").map(String::as_str),
            Some("text/plain")
        );
        assert!(
            validate_response_headers_2n18(&[
                ("Set-Cookie".to_owned(), "token=secret".to_owned(),)
            ])
            .is_err()
        );
        assert!(
            validate_response_headers_2n18(&[("X-Secret".to_owned(), "private".to_owned(),)])
                .is_err()
        );
    }

    #[test]
    fn invalid_values_and_duplicate_headers_fail_closed() {
        assert!(
            validate_response_headers_2n18(&[(
                "content-type".to_owned(),
                "text/plain\r\nCookie: a".to_owned(),
            )])
            .is_err()
        );
        assert!(
            validate_response_headers_2n18(&[
                ("content-type".to_owned(), "a".to_owned()),
                ("Content-Type".to_owned(), "b".to_owned()),
            ])
            .is_err()
        );
    }
}
