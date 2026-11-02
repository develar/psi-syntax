
#if false
    #define Z
    #if true
        #define Z
    #endif
#else
    #if Z
 ?1 // no errors here
    #endif
#endif
