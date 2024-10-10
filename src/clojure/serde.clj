(ns serde
  (:require [cheshire.core :as json])
  (:require [clojure.walk :as walk]))

(defn serialize-with-key-type [data]
  (json/generate-string
    (walk/postwalk
      (fn [x]
        (if (keyword? x)
          (str x)
          x))
      data)))


(defn deserialize-with-key-type [json-str]
  (let [data (json/parse-string json-str)]
    (walk/postwalk
      (fn [x]
        (cond
          (string? x) (if (.startsWith x ":")
                        (keyword (subs x 1))
                        x)
          (number? x) (long x)  ;; if this line not there, it will deserialize to Integer, 
                                ;; that causes problem in history.
          :else x))
      data)))

(defn deserialize-list-to-vec [json-str]
  (let [data (deserialize-with-key-type json-str)]
    (cond
     (vector? data) data
     (seq? data) (into [] data)
     :else data)))