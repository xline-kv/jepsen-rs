;; This clojure namespace provides custom serialization and deserialization 
;; functions that can keep the key-type value in clojure structure.
;; for more infomation, please see https://github.com/xline-kv/jepsen-rs/issues/10

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
         (number? x) (long x)   ;; if this line not there, it will deserialize to Integer, 
                                ;; that causes problem in history.
                                ;; And there are no float/double in history, so this conversion is ok.
         :else x))
     data)))

(defn deserialize-list-to-vec [json-str]
  (let [data (deserialize-with-key-type json-str)]
    (cond
      (seq? data) (into [] data)
      :else data)))
