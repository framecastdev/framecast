## 1. Notation

### 1.1 Type Notation

| Symbol | Meaning |
|--------|---------|
| `T` | Required field of type T |
| `T?` | Optional field of type T (nullable) |
| `T[]` | Array of type T |
| `T!` | Unique constraint |
| `T PK` | Primary key |
| `T FK → E` | Foreign key referencing entity E |
| `T DEFAULT v` | Default value v |
| `{a \| b \| c}` | Enumeration with values a, b, c |

### 1.2 Cardinality Notation

| Symbol | Meaning |
|--------|---------|
| `1` | Exactly one |
| `0..1` | Zero or one |
| `0..*` | Zero or many |
| `1..*` | One or many |
| `A ──── B` | Association |
| `A ◆─── B` | Composition (B cannot exist without A) |
| `A ◇─── B` | Aggregation (B can exist without A) |

### 1.3 Constraint Notation

| Symbol | Meaning |
|--------|---------|
| `∀` | For all |
| `∃` | There exists |
| `∄` | There does not exist |
| `⊕` | Exclusive or (exactly one) |
| `→` | Implies |
| `∧` | Logical and |
| `∨` | Logical or |
| `¬` | Logical not |
| `∈` | Element of |
| `∉` | Not element of |
| `≡` | Equivalent to / defined as |

### 1.4 State Transition Notation

```
State1 ──[event/guard]──► State2
         └─[action]
```

- **event**: What triggers the transition
- **guard**: Condition that must be true (in brackets)
- **action**: Side effect executed during transition

---
