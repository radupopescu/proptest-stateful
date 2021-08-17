# User guide

*Testing cannot prove the absence of bugs, but it prove their existence.*

## Introduction

Writing tests for a software system typically involves manually writing test
cases, where the behaviour of various components is examined by checking the
actual response of the component to a specific input against an expected
response. The limitation of this approach is that the person writing the tests
needs to identify useful input values for test cases, covering any boundary
conditions or corner cases of the system.

Property-based testing (PBT) provides an alternative: instead of constructing
test suites by finding relevant examples, the person writing the tests tries to
identify the invariants of the system-under-test (SUT), the properties which
hold for all input values. As a simple example, for a function which reverses a
sequence of elements, applying it twice would produce the original sequence:

```
reverse(reverse(A)) = A, for any sequence A
```

Property-based testing libraries typically provide a few things. First, they
facilitate the generation of random values which are the inputs to the different
property-based test cases written by the user. Generators are provided for
primitive values, which can be composed to generate random values of more
complex types. The other important functionality offered by property-based
testing libraries is used when a randomly-generated value invalidates the
property being tested. Most libraries implement a mechanism ("shrinking") to
search for, starting from the failing value, the minimal value which invalidates
the property in question.

In the case of scalar values, the shrinking process is implemented in terms of a
binary search, while in the case of more complex values, shrinking is performed
across multiple parameters. For example, a property-test taking a sequence of
numbers as input, may first attempt to shrink the sequence by removing elements.
Once that is no longer possible, it may attempt to shrink the individual values
remaining in the sequence. The shrinking strategy is derived automatically by
the library from the strategy to generate random values. Each combination of
random value generators maps to a phase of the shrinking process.

## Property-testing of stateful systems

## API overview