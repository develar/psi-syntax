
#if false
    #define Z
#else
    #if false
        #define Z
    #elif true
        #if Z
     ?1 // no errors here
        #endif
    #endif
#endif
