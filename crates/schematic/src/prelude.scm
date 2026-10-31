(define (identity x) x)

(define (constantly x)
  (lambda (& ignored) x))

(define (complement f)
  (lambda (& args) (not (apply f args))))

(define (partial f & fixed)
  (lambda (& more) (apply f (concat fixed more))))

(define (comp & fs)
  (if (empty? fs)
      identity
      (let ((fs (reverse fs)))
        (lambda (& args)
          (let loop ((acc (apply (first fs) args))
                     (remaining (rest fs)))
            (if (empty? remaining)
                acc
                (loop ((first remaining) acc) (rest remaining))))))))

(define (juxt & fs)
  (lambda (& args)
    (map (lambda (f) (apply f args)) (vec fs))))
