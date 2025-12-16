# MCP Langbase Reasoning - Tool Validation Prompts

Copy and paste these prompts to validate each tool.

---

## Phase 1-2 Tools (8 tools)

### 1. reasoning_linear
```
Use reasoning_linear with:
- content: "What are the benefits of modular software architecture?"
- confidence: 0.8
```

### 2. reasoning_tree
```
Use reasoning_tree with:
- content: "Should we use SQL or NoSQL for our new application?"
- num_branches: 3
```

### 3. reasoning_tree_list
```
Use reasoning_tree_list with:
- session_id: [SESSION_ID from reasoning_tree above]
```

### 4. reasoning_tree_focus
```
Use reasoning_tree_focus with:
- session_id: [SESSION_ID from reasoning_tree]
- branch_id: [BRANCH_ID from reasoning_tree child_branches]
```

### 5. reasoning_tree_complete
```
Use reasoning_tree_complete with:
- branch_id: [BRANCH_ID from reasoning_tree_focus]
- completed: true
```

### 6. reasoning_divergent
```
Use reasoning_divergent with:
- content: "How might AI change software development in the next 5 years?"
- num_perspectives: 3
- challenge_assumptions: true
```

### 7. reasoning_reflection
```
Use reasoning_reflection with:
- content: "All startups should use microservices because they scale better."
- max_iterations: 2
- quality_threshold: 0.7
```

### 8. reasoning_reflection_evaluate
```
Use reasoning_reflection_evaluate with:
- session_id: [SESSION_ID from reasoning_linear]
```

---

## Phase 3 Tools (12 tools)

### 9. reasoning_auto
```
Use reasoning_auto with:
- content: "Compare the trade-offs between REST and GraphQL APIs"
```

### 10. reasoning_checkpoint_create
```
Use reasoning_checkpoint_create with:
- session_id: [SESSION_ID from reasoning_linear]
- name: "validation-checkpoint"
- description: "Testing checkpoint creation"
```

### 11. reasoning_checkpoint_list
```
Use reasoning_checkpoint_list with:
- session_id: [SESSION_ID from reasoning_linear]
```

### 12. reasoning_backtrack
```
Use reasoning_backtrack with:
- checkpoint_id: [CHECKPOINT_ID from reasoning_checkpoint_create]
- new_direction: "Explore a different approach"
```

### 13. reasoning_got_init
```
Use reasoning_got_init with:
- content: "What strategies improve code maintainability?"
- session_id: "got-validation-test"
```

### 14. reasoning_got_generate
```
Use reasoning_got_generate with:
- session_id: "got-validation-test"
- node_id: [ROOT_NODE_ID from reasoning_got_init]
- k: 3
- problem: "What strategies improve code maintainability?"
```

### 15. reasoning_got_score
```
Use reasoning_got_score with:
- session_id: "got-validation-test"
- node_id: [NODE_ID from reasoning_got_generate continuations]
- problem: "What strategies improve code maintainability?"
```

### 16. reasoning_got_refine
```
Use reasoning_got_refine with:
- session_id: "got-validation-test"
- node_id: [NODE_ID from reasoning_got_score]
- problem: "What strategies improve code maintainability?"
```

### 17. reasoning_got_aggregate
```
Use reasoning_got_aggregate with:
- session_id: "got-validation-test"
- node_ids: [ARRAY of 2-3 NODE_IDs from reasoning_got_generate]
- problem: "What strategies improve code maintainability?"
```

### 18. reasoning_got_prune
```
Use reasoning_got_prune with:
- session_id: "got-validation-test"
- threshold: 0.3
```

### 19. reasoning_got_finalize
```
Use reasoning_got_finalize with:
- session_id: "got-validation-test"
- terminal_node_ids: [AGGREGATED_NODE_ID from reasoning_got_aggregate]
```

### 20. reasoning_got_state
```
Use reasoning_got_state with:
- session_id: "got-validation-test"
```

---

## Quick Full Validation Sequence

Run these in order for complete validation:

```
1. reasoning_linear: content="Test linear reasoning"
2. reasoning_auto: content="Analyze microservices vs monolith"
3. reasoning_tree: content="Compare caching strategies", num_branches=3
4. reasoning_divergent: content="Creative solutions for scaling", num_perspectives=3
5. reasoning_reflection: content="We should always use the cloud"
6. reasoning_checkpoint_create: session_id=[from step 1], name="test"
7. reasoning_checkpoint_list: session_id=[from step 1]
8. reasoning_got_init: content="Problem solving strategies", session_id="quick-test"
9. reasoning_got_generate: session_id="quick-test", node_id=[root], k=2
10. reasoning_got_state: session_id="quick-test"
```
