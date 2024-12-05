# XiaoXuan Native Assembly

An assembly language for XiaoXuan Native programming language.

The compilation pipeline:

```text
XiaoXuan Native -> 
    XiaoXuan Native IR -> 
    XiaoXuan Native Assembly -> 
    CodeGen -> 
    Object File
```

The linking pipeline:

```text
Object Files -> Shared Modules/Share Libraries/Applications
```
